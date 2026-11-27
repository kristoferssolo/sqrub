//! Finding the Rust files to format

use ignore::{DirEntry, Error, WalkBuilder};
use std::path::{Path, PathBuf};

/// Rust files under `inputs`, in file name order within each directory
///
/// Directories are searched recursively, skipping hidden paths and paths
/// excluded by `.gitignore`, `.ignore` and similar files. Files named in
/// `inputs` are always included. Errors, such as a missing input, are yielded
/// in place of the paths they affect.
pub fn rust_files<P: AsRef<Path>>(
    inputs: &[P],
) -> impl Iterator<Item = Result<PathBuf, Error>> + use<P> {
    let mut paths = inputs.iter().map(AsRef::as_ref);
    let mut builder = WalkBuilder::new(paths.next().unwrap_or_else(|| Path::new(".")));
    for path in paths {
        builder.add(path);
    }
    builder.sort_by_file_name(Ord::cmp);

    builder.build().filter_map(|entry| match entry {
        Ok(entry) => is_rust_file(&entry).then(|| Ok(entry.into_path())),
        Err(error) => Some(Err(error)),
    })
}

fn is_rust_file(entry: &DirEntry) -> bool {
    entry.file_type().is_some_and(|kind| kind.is_file())
        && entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "rs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::{assert_err, assert_none, assert_ok, assert_ok_eq, assert_some};
    use std::fs;
    use tempfile::TempDir;

    fn tree(files: &[&str]) -> TempDir {
        let root = assert_ok!(TempDir::new());
        for file in files {
            let path = root.path().join(file);
            assert_ok!(fs::create_dir_all(path.parent().unwrap()));
            assert_ok!(fs::write(path, ""));
        }
        root
    }

    fn found<P: AsRef<Path>>(root: &TempDir, inputs: &[P]) -> Vec<PathBuf> {
        rust_files(inputs)
            .map(|path| assert_ok!(path))
            .map(|path| assert_ok!(path.strip_prefix(root.path())).to_owned())
            .collect()
    }

    #[test]
    fn finds_nested_rust_files_in_order() {
        let root = tree(&["src/main.rs", "src/b/mod.rs", "src/a.rs", "README.md"]);
        assert_eq!(
            found(&root, &[root.path()]),
            [
                PathBuf::from("src/a.rs"),
                PathBuf::from("src/b/mod.rs"),
                PathBuf::from("src/main.rs"),
            ]
        );
    }

    #[test]
    fn skips_ignored_and_hidden_paths() {
        let root = tree(&["src/main.rs", "target/debug/build.rs", ".hidden/x.rs"]);
        assert_ok!(fs::write(root.path().join(".ignore"), "target/\n"));
        assert_eq!(found(&root, &[root.path()]), [PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn includes_named_files_even_when_ignored() {
        let root = tree(&["target/generated.rs"]);
        assert_ok!(fs::write(root.path().join(".ignore"), "target/\n"));
        let named = root.path().join("target/generated.rs");
        assert_eq!(
            found(&root, &[named]),
            [PathBuf::from("target/generated.rs")]
        );
    }

    #[test]
    fn reports_missing_inputs() {
        let root = tree(&["src/main.rs"]);
        let mut results = rust_files(&[root.path().join("missing"), root.path().join("src")]);
        assert_err!(assert_some!(results.next()));
        assert_ok_eq!(
            assert_some!(results.next()),
            root.path().join("src/main.rs")
        );
        assert_none!(results.next());
    }
}
