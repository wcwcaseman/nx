use std::path::{Path, PathBuf};
use std::thread;
use std::thread::available_parallelism;

use crossbeam_channel::unbounded;
use ignore::{WalkBuilder};
use tracing::trace;

use crate::native::glob::build_glob_set;

use crate::native::utils::{get_mod_time, Normalize};
use walkdir::WalkDir;

#[derive(PartialEq, Debug, Ord, PartialOrd, Eq, Clone)]
pub struct NxFile {
    pub full_path: String,
    pub normalized_path: String,
    pub mod_time: i64,
}

/// Walks the directory in a single thread and does not ignore any files
/// Should only be used for small directories, and not traversing the whole workspace
///
/// The `ignores` argument is used to filter entries. This is important to make sure that any ignore globs are applied on the `filter_entry` function
pub fn nx_walker_sync<'a, P>(
    directory: P,
    ignores: Option<&[String]>,
) -> impl Iterator<Item = PathBuf>
where
    P: AsRef<Path> + 'a,
{
    let base_dir: PathBuf = directory.as_ref().into();

    let mut base_ignores: Vec<String> = vec![
        "**/node_modules".into(),
        "**/.git".into(),
        "**/.nx/cache".into(),
        "**/.nx/workspace-data".into(),
        "**/.yarn/cache".into(),
    ];

    if let Some(additional_ignores) = ignores {
        base_ignores.extend(additional_ignores.iter().map(|s| format!("**/{}", s)));
    };

    let ignore_glob_set = build_glob_set(&base_ignores).expect("Should be valid globs");

    // Use WalkDir instead of ignore::WalkBuilder because it's faster
    WalkDir::new(&base_dir)
        .into_iter()
        .filter_entry(move |entry| {
            let path = entry.path().to_string_lossy();
            !ignore_glob_set.is_match(path.as_ref())
        })
        .filter_map(move |entry| {
            entry
                .ok()
                .and_then(|e| e.path().strip_prefix(&base_dir).ok().map(|p| p.to_owned()))
        })
}

/// Walks the directory in a single thread and does not ignore any files
/// This should only be used in wasm environments
// #[cfg(target_arch = "wasm32")]
// pub fn nx_walker_sync_with_ignore<'a, P>(
//     directory: P
// ) -> impl Iterator<Item = NxFile>
//     where
//         P: AsRef<Path> + 'a,
// {
//     let base_dir: PathBuf = directory.as_ref().into();
//
//     let base_ignores: Vec<String> = vec![
//         "**/node_modules".into(),
//         "**/.git".into(),
//         "**/.nx/cache".into(),
//         "**/.nx/workspace-data".into(),
//         "**/.yarn/cache".into(),
//     ];
//
//     let ignore_glob_set = build_glob_set(&base_ignores).expect("Should be valid globs");
//
//
//     let mut ignore_builder = ignore::gitignore::GitignoreBuilder::new(&base_dir);
//
//
//     ignore_builder.add(base_dir.join(".gitignore"));
//     ignore_builder.add(base_dir.join(".nxignore"));
//     let ignore = ignore_builder.build().unwrap();
//
//
//     let base_dir_clone = base_dir.clone();
//     // Use WalkDir instead of ignore::WalkBuilder because it's faster
//     WalkDir::new(&base_dir)
//         .into_iter()
//         .filter_entry(move |entry| {
//             let path = entry.path();
//
//             let is_dir = path.is_dir();
//
//             !matches!(
//                             ignore.matched_path_or_any_parents(&path.strip_prefix(&base_dir_clone).unwrap(), is_dir),
//                             ignore::Match::Ignore(_)
//                         ) && !ignore_glob_set.is_match(path)
//         })
//         .filter_map(move |entry| {
//             entry
//                 .ok()
//                 .and_then(|e| {
//                     let path = e.path();
//                     let is_dir = path.is_dir();
//
//                     if is_dir {
//                         return None;
//                     }
//
//                     let Ok(relative_path) = e.path().strip_prefix(&base_dir) else {
//                         return None;
//                     };
//                     let Ok(metadata) = e.metadata() else {
//                         return None;
//                     };
//
//                     let normalized_path = relative_path.to_normalized_string();
//                     let mod_time = get_mod_time(&metadata);
//                     trace!("Walked {}", &normalized_path);
//
//                     Some(NxFile {
//                         full_path: String::from(e.path().to_string_lossy()),
//                         normalized_path,
//                         mod_time,
//                     })
//                 })
//         })
// }

#[cfg(target_arch = "wasm32")]
pub fn nx_walker_sync_with_ignore<P>(directory: P) -> impl Iterator<Item = NxFile>
where
    P: AsRef<Path>,
{
    let directory: PathBuf = directory.as_ref().into();

    let ignore_glob_set = build_glob_set(&[
        "**/node_modules",
        "**/.git",
        "**/.nx/cache",
        "**/.nx/workspace-data",
        "**/.yarn/cache",
    ])
        .expect("These static ignores always build");

    let mut walker = WalkBuilder::new(&directory);
    walker.hidden(false);
    walker.add_custom_ignore_filename(".nxignore");

    // We should make sure to always ignore node_modules and the .git folder
    walker.filter_entry(move |entry| {
        let path = entry.path().to_string_lossy();
        !ignore_glob_set.is_match(path.as_ref())
    });

    let entries = walker.build();

    entries.filter_map(move |entry| {
        let Ok(dir_entry) = entry else {
            return None;
        };

        if dir_entry.file_type().is_some_and(|d| d.is_dir()) {
            return None;
        }

        let Ok(file_path) = dir_entry.path().strip_prefix(&directory) else {
            return None;
        };

        let Ok(metadata) = dir_entry.metadata() else {
            return None;
        };

        Some(NxFile {
            full_path: String::from(dir_entry.path().to_string_lossy()),
            normalized_path: file_path.to_normalized_string(),
            mod_time: get_mod_time(&metadata),
        })
    })
}

/// Walk the directory and ignore files from .gitignore and .nxignore
pub fn nx_walker<P>(directory: P) -> impl Iterator<Item = NxFile>
where
    P: AsRef<Path>,
{
    let directory = directory.as_ref();

    let ignore_glob_set = build_glob_set(&[
        "**/node_modules",
        "**/.git",
        "**/.nx/cache",
        "**/.nx/workspace-data",
        "**/.yarn/cache",
    ])
    .expect("These static ignores always build");

    let mut walker = WalkBuilder::new(directory);
    walker.hidden(false);
    walker.add_custom_ignore_filename(".nxignore");

    // We should make sure to always ignore node_modules and the .git folder
    walker.filter_entry(move |entry| {
        let path = entry.path().to_string_lossy();
        !ignore_glob_set.is_match(path.as_ref())
    });

    let cpus = available_parallelism().map_or(2, |n| n.get()) - 1;

    let (sender, receiver) = unbounded();

    trace!(?directory, "walking");

    let now = std::time::Instant::now();
    walker.threads(cpus).build_parallel().run(|| {
        let tx = sender.clone();
        Box::new(move |entry| {
            use ignore::WalkState::*;

            let Ok(dir_entry) = entry else {
                return Continue;
            };

            if dir_entry.file_type().is_some_and(|d| d.is_dir()) {
                return Continue;
            }

            let Ok(file_path) = dir_entry.path().strip_prefix(directory) else {
                return Continue;
            };

            let Ok(metadata) = dir_entry.metadata() else {
                return Continue;
            };

            tx.send(NxFile {
                full_path: String::from(dir_entry.path().to_string_lossy()),
                normalized_path: file_path.to_normalized_string(),
                mod_time: get_mod_time(&metadata),
            })
            .ok();

            Continue
        })
    });
    trace!("walked in {:?}", now.elapsed());

    let receiver_thread = thread::spawn(move || receiver.into_iter());
    drop(sender);
    receiver_thread.join().unwrap()
}

#[cfg(test)]
mod test {
    use std::{assert_eq, vec};

    use assert_fs::prelude::*;
    use assert_fs::TempDir;

    use super::*;

    ///
    /// Setup a temporary directory to do testing in
    ///
    fn setup_fs() -> TempDir {
        let temp = TempDir::new().unwrap();
        temp.child("test.txt").write_str("content").unwrap();
        temp.child("foo.txt").write_str("content1").unwrap();
        temp.child("bar.txt").write_str("content2").unwrap();
        temp.child("baz")
            .child("qux.txt")
            .write_str("content@qux")
            .unwrap();
        temp.child("node_modules")
            .child("node-module-dep")
            .write_str("content")
            .unwrap();
        temp
    }

    #[test]
    fn it_walks_a_directory() {
        // handle empty workspaces
        let content = nx_walker("/does/not/exist").collect::<Vec<_>>();
        assert!(content.is_empty());

        let temp_dir = setup_fs();

        let mut content = nx_walker(&temp_dir).collect::<Vec<_>>();
        content.sort();
        let content = content
            .into_iter()
            .map(|f| (f.full_path.into(), f.normalized_path.into()))
            .collect::<Vec<_>>();
        assert_eq!(
            content,
            vec![
                (temp_dir.join("bar.txt"), PathBuf::from("bar.txt")),
                (temp_dir.join("baz/qux.txt"), PathBuf::from("baz/qux.txt")),
                (temp_dir.join("foo.txt"), PathBuf::from("foo.txt")),
                (temp_dir.join("test.txt"), PathBuf::from("test.txt")),
            ]
        );
    }

    #[test]
    fn handles_nx_ignore() {
        let temp_dir = setup_fs();

        temp_dir
            .child("nested")
            .child("child.txt")
            .write_str("data")
            .unwrap();
        temp_dir
            .child("nested")
            .child("child-two")
            .child("grand_child.txt")
            .write_str("data")
            .unwrap();
        temp_dir
            .child("v1")
            .child("packages")
            .child("pkg-a")
            .child("pkg-a.txt")
            .write_str("data")
            .unwrap();
        temp_dir
            .child("v1")
            .child("packages")
            .child("pkg-b")
            .child("pkg-b.txt")
            .write_str("data")
            .unwrap();
        temp_dir
            .child("packages")
            .child("pkg-c")
            .child("pkg-c.txt")
            .write_str("data")
            .unwrap();

        // add nxignore file
        temp_dir
            .child(".nxignore")
            .write_str(
                r"baz/
nested/child.txt
nested/child-two/

# this should only ignore root level packages, not nested
/packages
    ",
            )
            .unwrap();

        let mut file_names = nx_walker(temp_dir)
            .map(
                |NxFile {
                     normalized_path: relative_path,
                     ..
                 }| relative_path,
            )
            .collect::<Vec<_>>();

        file_names.sort();

        assert_eq!(
            file_names,
            vec!(
                ".nxignore",
                "bar.txt",
                "foo.txt",
                "test.txt",
                "v1/packages/pkg-a/pkg-a.txt",
                "v1/packages/pkg-b/pkg-b.txt"
            )
        );
    }
}
