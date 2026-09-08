//! Builds the grammar set the widget reads code with, once, here rather than at run time.
//!
//! syntect can read a `.sublime-syntax` file straight off the disk, and for the two vendored
//! TypeScript grammars that costs the best part of two seconds — they are three and a half
//! thousand lines each, and the first `.ts` file opened would pay for it inside a frame. So
//! the YAML is parsed here, folded together with syntect's bundled grammars, and written out
//! as the binary dump syntect loads its own defaults from. Opening the first file then costs
//! about a millisecond.
//!
//! Doing it here rather than committing the dump also means the dump is always the work of
//! the same syntect the crate is being compiled against, which a checked-in binary could not
//! promise.

fn main() {
    #[cfg(feature = "syntax")]
    grammars::write_dump();
}

#[cfg(feature = "syntax")]
mod grammars {
    use std::{env, fs, path::PathBuf};

    use syntect::{
        dumps::dump_to_file,
        parsing::{SyntaxDefinition, SyntaxSet},
    };

    /// The vendored grammars, by the name of their file in `grammars/`.
    ///
    /// syntect's bundled set has no TypeScript of any kind in it — not `.ts`, not `.tsx` —
    /// and this is the whole of what is added to make up for that. See the notice at the top
    /// of either file for where it came from and under what licence.
    const VENDORED: &[&str] = &["TypeScript", "TypeScriptReact"];

    /// Fold the vendored grammars into syntect's bundled ones and write the lot to `OUT_DIR`.
    pub(super) fn write_dump() {
        // The newline variant, because the crate hands each line to the parser with its
        // newline still on it.
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        for name in VENDORED {
            let path = PathBuf::from("grammars").join(format!("{name}.sublime-syntax"));
            println!("cargo::rerun-if-changed={}", path.display());
            let yaml = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{} is not readable: {error}", path.display()));
            let syntax = SyntaxDefinition::load_from_str(&yaml, true, Some(name))
                .unwrap_or_else(|error| panic!("{} is not a grammar: {error}", path.display()));
            builder.add(syntax);
        }

        let out = PathBuf::from(env::var("OUT_DIR").expect("cargo runs us with an OUT_DIR"))
            .join("grammars.bin");
        dump_to_file(&builder.build(), &out).expect("the grammar dump could not be written");
    }
}
