// `sqlx::migrate!()` is a proc-macro that embeds the contents of `migrations/`
// at compile time.  Cargo does not, by default, re-run a proc-macro when a file
// inside a watched directory changes — only when source files of the crate
// itself change.  Adding the rerun-if-changed directive here forces a rebuild
// (and re-evaluation of the `migrate!()` macro) whenever a migration file is
// added, removed, or modified.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
