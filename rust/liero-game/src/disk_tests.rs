use super::*;

/// Runs `f` with this thread's files in a new tree, then back on disk.
fn in_tree(f: impl FnOnce()) {
    use_tree();
    f();
    use_disk();
}

#[test]
fn keys_skip_dots_and_go_up() {
    assert_eq!(key(b"").unwrap(), b"");
    assert_eq!(key(b".").unwrap(), b"");
    assert_eq!(key(b"./a//b/./c/").unwrap(), b"a/b/c");
    assert_eq!(key(b"/a/b/../c").unwrap(), b"a/c");
    assert_eq!(key(b"a/.."), Some(Vec::new()));
    assert_eq!(key(b".."), None);
    assert_eq!(key(b"a/\xff"), None);
}

#[test]
fn files_need_their_folder() {
    in_tree(|| {
        assert!(exists(b"."));
        assert_eq!(is_dir(b"./"), Some(true));
        assert!(!write(b"a/x", b"1"));
        assert!(!create_dir(b"a/b"));
        assert!(create_dir(b"./a"));
        assert!(!create_dir(b"a"), "already there");
        assert!(create_dir(b"a/b"));
        assert!(write(b"./a/b/x", b"12"));
        assert_eq!(is_dir(b"a/b/x"), Some(false));
        assert_eq!(is_dir(b"a/b/y"), None);
        // Not in a file, and not over a folder.
        assert!(!write(b"a/b/x/y", b"3"));
        assert!(!create_dir(b"a/b/x/y"));
        assert!(!write(b"a/b", b"3"));
        assert!(!create(b"a"));
        assert_eq!(read(b"a/b/x").unwrap(), b"12");
    });
}

#[test]
fn reading_folders_and_files() {
    in_tree(|| {
        assert!(create_dir(b"d"));
        assert!(write(b"d/f", b"abc"));
        // A folder opens as nothing but doesn't read whole.
        assert_eq!(open_read(b"d").unwrap(), b"");
        assert_eq!(read(b"d"), None);
        assert_eq!(open_read(b"d/f").unwrap(), b"abc");
        assert_eq!(open_read(b"d/g"), None);
        assert_eq!(read(b"d/g"), None);
        // Names are case-sensitive, as on Linux.
        assert_eq!(open_read(b"D/F"), None);
    });
}

#[test]
fn writing_replaces_and_appending_adds() {
    in_tree(|| {
        assert!(!append(b"f", b"x"), "not there yet");
        assert!(create(b"f"));
        assert!(append(b"f", b"ab"));
        assert!(append(b"./f", b"c"));
        assert_eq!(read(b"f").unwrap(), b"abc");
        assert!(write(b"f", b"z"));
        assert_eq!(read(b"f").unwrap(), b"z");
        assert!(create(b"f"));
        assert_eq!(read(b"f").unwrap(), b"");
        assert!(create_dir(b"d"));
        assert!(!append(b"d", b"x"));
    });
}

#[test]
fn listing_gives_only_a_folders_own_names() {
    in_tree(|| {
        for d in [&b"a"[..], b"a/b", b"ab", b"a/b/c"] {
            assert!(create_dir(d));
        }
        for f in [&b"a/x"[..], b"a/b/y", b"ab/z", b"top"] {
            assert!(write(f, b""));
        }
        let sorted = |p: &[u8]| {
            let mut v = list(p).unwrap();
            v.sort();
            v
        };
        assert_eq!(sorted(b"."), [&b"a"[..], b"ab", b"top"]);
        assert_eq!(sorted(b"a"), [&b"b"[..], b"x"]);
        assert_eq!(sorted(b"./a/b/"), [&b"c"[..], b"y"]);
        assert_eq!(sorted(b"a/b/c"), Vec::<Vec<u8>>::new());
        assert_eq!(list(b"a/x"), None);
        assert_eq!(list(b"nothing"), None);
    });
}

#[test]
fn removing_takes_only_files() {
    in_tree(|| {
        assert!(create_dir(b"d"));
        assert!(write(b"d/f", b"1"));
        assert!(!remove_file(b"d"));
        assert!(!remove_file(b"d/g"));
        assert!(remove_file(b"d/f"));
        assert!(!exists(b"d/f"));
        assert!(exists(b"d"));
    });
}

#[test]
fn the_tree_is_this_threads_and_lists_its_files() {
    in_tree(|| {
        assert!(uses_tree());
        assert!(create_dir(b"d"));
        assert!(write(b"d/f", b"1"));
        assert!(write(b"e", b"2"));
        assert_eq!(tree_files(), [(b"d/f".to_vec(), b"1".to_vec()), (b"e".to_vec(), b"2".to_vec())]);
        std::thread::spawn(|| assert!(!uses_tree())).join().unwrap();
        // A new tree starts empty.
        use_tree();
        assert!(tree_files().is_empty());
    });
    assert!(!uses_tree());
    assert!(tree_files().is_empty());
}

/// The file layer on the tree: writing a file and its folders, the case
/// tries and the listing, as `fs_tests.rs` checks on disk.
#[test]
fn the_file_layer_works_on_the_tree() {
    use crate::fs::Node;
    in_tree(|| {
        assert!(create_dir(b"cfg"));
        let node = Node::new(b"/cfg/Replays/x.lrp");
        assert!(node.write(b"data"), "makes the folder");
        assert_eq!(Node::new(b"cfg/Replays/x.lrp").read().unwrap(), b"data");
        assert_eq!(Node::new(b"cfg/REPLAYS/X.LRP").read(), None, "the case tries change the whole path");
        let names: Vec<_> = Node::new(b"cfg").list().into_iter().map(|e| (e.name, e.is_dir)).collect();
        assert_eq!(names, [(b"Replays".to_vec(), true)]);
    });
}
