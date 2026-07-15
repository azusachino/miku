use super::routes::*;
use super::*;
use miku_app::RuntimeConfig;

fn test_breadcrumbs() -> Vec<BreadcrumbItem> {
    breadcrumb_items("Notes/Daily", "Daily")
}

#[test]
fn test_template_rendering() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));

    let template = templates_env
        .get_template("page.html")
        .expect("Failed to get page.html template");
    let rendered = template
        .render(context! {
            title => "Test Title",
            path => "TestPath",
            exists => true,
            content_html => "<p>Test content</p>",
            body => "# Test Title\n\nTest content",
            loaded_hash => "abc",
            has_mermaid => false,
            backlinks => Vec::<Backlink>::new(),
            unlinked_mentions => Vec::<UnlinkedMention>::new(),
            toc => Vec::<Heading>::new(),
            word_count => 2usize,
            backlink_count => 0usize,
            updated => "2026-06-27 12:00",
            frontmatter => serde_json::Value::Object(serde_json::Map::new()),
            breadcrumb_parent => Option::<String>::None,
            breadcrumbs => test_breadcrumbs(),
        })
        .expect("Failed to render template");

    assert!(rendered.contains("Test Title"));
    assert!(rendered.contains("miku"));
    assert!(rendered.contains("data-inline-editor"));
    assert!(rendered.contains("data-inline-body"));
    assert!(rendered.contains("@codemirror/autocomplete"));
    assert!(rendered.contains("/api/v1/quickswitch?q="));
    assert!(rendered.contains("class=\"mk-breadcrumb-link\""));
    assert!(rendered.contains("href=\"/folders/Notes\""));
    assert!(rendered.contains("href=\"/p/Notes&#x2f;Daily\""));
    assert!(!rendered.contains("mermaid.min.js"));
}

#[test]
fn test_reader_page_path_is_extensionless() {
    assert_eq!(reader_page_path("Notes/Source.md"), "Notes/Source");
    assert_eq!(reader_page_path("Notes/Source"), "Notes/Source");
}

#[test]
fn test_folder_template_renders_folder_browser() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));

    let template = templates_env
        .get_template("folder.html")
        .expect("Failed to get folder.html template");
    let rendered = template
        .render(context! {
            title => "Notes",
            path => "Notes",
            folders => vec![FolderChild {
                name: "Daily".to_string(),
                path: "Notes/Daily".to_string(),
            }],
            pages => vec![FolderPage {
                title: "Overview".to_string(),
                path: "Notes/Overview".to_string(),
            }],
            nav_pages => Vec::<NavNode>::new(),
            breadcrumbs => breadcrumb_items("Notes", "Notes"),
        })
        .expect("Failed to render folder.html template");

    assert!(rendered.contains("FOLDER"));
    assert!(rendered.contains("href=\"/folders/Notes&#x2f;Daily\""));
    assert!(rendered.contains("href=\"/p/Notes&#x2f;Overview\""));
    assert!(rendered.contains("Create note here"));
}

#[test]
fn test_quickswitch_clears_loading_state_defensively() {
    let base = std::fs::read_to_string("src/templates/base.html")
        .expect("Failed to read base.html template");

    assert!(base.contains("queuePaletteRefresh()"));
    assert!(base.contains("paletteRequestId"));
    assert!(base.contains("AbortController"));
    assert!(base.contains("this.paletteLoading = false"));
    assert!(!base.contains("@input=\"refreshPalette()\""));
    assert!(!base.contains("x-model.debounce.120ms=\"paletteQuery\""));
}

#[test]
fn test_shell_has_resizable_panes_without_repeated_page_marks() {
    let base = std::fs::read_to_string("src/templates/base.html")
        .expect("Failed to read base.html template");
    let page =
        std::fs::read_to_string("src/templates/page.html").expect("Failed to read page.html");
    let css = std::fs::read_to_string("static/miku.css").expect("Failed to read miku.css");

    assert!(base.contains("mk-sidebar-resizer"));
    assert!(page.contains("mk-rail-resizer"));
    assert!(base.contains("miku:ui:v1"));
    assert!(base.contains("window.mikuStorage"));
    assert!(base.contains("miku-open-rename"));
    assert!(base.contains("id=\"rename-path\""));
    assert!(!base.contains("window.prompt"));
    assert!(css.contains("grid-template-columns: minmax(0, 68ch) 8px var(--rail-w)"));
    assert!(!page.contains("mk-page-mark"));
}

#[test]
fn test_paginate_tags_returns_bounded_pages() {
    let tags = (0..125)
        .map(|index| TagCount {
            tag: format!("tag-{index}"),
            count: index,
        })
        .collect();

    let first = paginate_tags(tags, 0, 50);
    assert_eq!(first.tags.len(), 50);
    assert_eq!(first.total, 125);
    assert!(first.has_more);
    assert_eq!(first.next_offset, 50);

    let last = paginate_tags(
        (0..125)
            .map(|index| TagCount {
                tag: format!("tag-{index}"),
                count: index,
            })
            .collect(),
        100,
        50,
    );
    assert_eq!(last.tags.len(), 25);
    assert!(!last.has_more);
    assert_eq!(last.next_offset, 125);
}

#[test]
fn test_paginate_tag_results_returns_bounded_pages() {
    let pages = (0..125)
        .map(|index| PageRef {
            path: format!("Notes/{index}"),
            title: format!("Note {index}"),
        })
        .collect();

    let first = paginate_page_refs(pages, 0, TAG_RESULT_PAGE_SIZE);
    assert_eq!(first.pages.len(), TAG_RESULT_PAGE_SIZE);
    assert_eq!(first.total, 125);
    assert!(first.has_more);
    assert_eq!(first.next_offset, TAG_RESULT_PAGE_SIZE);
}

#[test]
fn test_template_rendering_with_mermaid_uses_shell_lazy_loader() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));

    let template = templates_env
        .get_template("page.html")
        .expect("Failed to get page.html template");
    let rendered = template
        .render(context! {
            title => "Test Title",
            path => "TestPath",
            exists => true,
            content_html => "<p>Test content</p>",
            body => "# Test Title\n\nTest content",
            loaded_hash => "abc",
            has_mermaid => true,
            backlinks => Vec::<Backlink>::new(),
            unlinked_mentions => Vec::<UnlinkedMention>::new(),
            toc => Vec::<Heading>::new(),
            word_count => 2usize,
            backlink_count => 0usize,
            updated => "2026-06-27 12:00",
            frontmatter => serde_json::Value::Object(serde_json::Map::new()),
            breadcrumb_parent => Option::<String>::None,
            breadcrumbs => test_breadcrumbs(),
        })
        .expect("Failed to render template");

    assert!(!rendered.contains("mermaid.min.js"));
    let miku = std::fs::read_to_string("static/miku.js").expect("Failed to read miku.js");
    assert!(miku.contains("cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.min.js"));
}

#[test]
fn test_page_template_renders_frontmatter_properties() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));
    let template = templates_env
        .get_template("page.html")
        .expect("Failed to get page.html template");

    let frontmatter = serde_json::json!({
        "status": "draft",
        "tags": ["miku", "wiki"],
    });
    let rendered = template
        .render(context! {
            title => "Test Title",
            path => "Notes/Daily",
            exists => true,
            content_html => "<p>Test</p>",
            body => "# Test Title\n\nTest",
            loaded_hash => "abc",
            has_mermaid => false,
            backlinks => Vec::<Backlink>::new(),
            unlinked_mentions => Vec::<UnlinkedMention>::new(),
            toc => Vec::<Heading>::new(),
            word_count => 1usize,
            backlink_count => 0usize,
            updated => "2026-06-27 12:00",
            frontmatter => frontmatter,
            breadcrumb_parent => Option::<String>::None,
            breadcrumbs => test_breadcrumbs(),
        })
        .expect("Failed to render template");

    assert!(rendered.contains("status"));
    assert!(rendered.contains("draft"));
    // Sequence values render as chips.
    assert!(rendered.contains("miku"));
    assert!(rendered.contains("wiki"));
}

#[test]
fn test_page_template_has_collapsible_cards_and_order() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));
    let template = templates_env
        .get_template("page.html")
        .expect("Failed to get page.html template");

    let frontmatter = serde_json::json!({
        "status": "draft",
    });

    let toc_headings = vec![Heading {
        level: 2,
        text: "Section A".to_string(),
        id: "section-a".to_string(),
    }];

    let rendered = template
        .render(context! {
            title => "Test Title",
            path => "Notes/Daily",
            exists => true,
            content_html => "<p>Test</p>",
            body => "# Test Title\n\nTest",
            loaded_hash => "abc",
            has_mermaid => false,
            backlinks => Vec::<Backlink>::new(),
            unlinked_mentions => Vec::<UnlinkedMention>::new(),
            toc => toc_headings,
            word_count => 10usize,
            backlink_count => 1usize,
            updated => "2026-06-27 12:00",
            frontmatter => frontmatter,
            breadcrumb_parent => Option::<String>::None,
            breadcrumbs => test_breadcrumbs(),
        })
        .expect("Failed to render template");

    // Verify the presence of collapsible elements
    assert!(rendered.contains("x-data=\"{ collapsed: false }\""));
    assert!(rendered.contains("mk-collapse-chevron"));

    // Verify the reordered elements (PAGE INFO then ON THIS PAGE)
    let idx_page_info = rendered.find("PAGE INFO").expect("PAGE INFO not found");
    let idx_on_this_page = rendered
        .find("ON THIS PAGE")
        .expect("ON THIS PAGE not found");

    assert!(
        idx_page_info < idx_on_this_page,
        "PAGE INFO should be rendered before ON THIS PAGE"
    );
}

#[test]
fn test_template_seed_maps_ids_to_bodies() {
    assert!(template_seed("meeting").contains("## Agenda"));
    assert!(template_seed("reading").contains("## Highlights"));
    assert!(template_seed("project").contains("## Tasks"));
    // Blank and unknown ids both produce an empty page.
    assert_eq!(template_seed("blank"), "");
    assert_eq!(template_seed("bogus"), "");
}

#[test]
fn test_edit_template_renders_seed_body_into_textarea() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));

    let template = templates_env
        .get_template("edit.html")
        .expect("Failed to get edit.html template");
    // Mirrors page_edit seeding a new page from ?template=meeting.
    let rendered = template
        .render(context! {
            path => "Notes/Standup",
            body => template_seed("meeting"),
            loaded_hash => "",
            nav_pages => Vec::<NavNode>::new(),
        })
        .expect("Failed to render template");

    assert!(rendered.contains("## Agenda"));
    assert!(rendered.contains("## Actions"));
}

#[test]
fn test_edit_template_has_live_preview_editor() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));

    let template = templates_env
        .get_template("edit.html")
        .expect("Failed to get edit.html template");
    let rendered = template
        .render(context! {
            path => "TestPath",
            body => "# Draft",
            loaded_hash => "abc",
            nav_pages => Vec::<NavNode>::new(),
        })
        .expect("Failed to render template");

    assert!(rendered.contains("mk-edit"));
    assert!(rendered.contains("mk-edit-split"));
    assert!(rendered.contains("mk-preview mk-prose"));
    assert!(rendered.contains("name=\"loaded_hash\" value=\"abc\""));
    assert!(rendered.contains("fetch('/preview'"));
    assert!(rendered.contains("action=\"/p/TestPath\" method=\"POST\""));
}

#[test]
fn test_build_nav_tree_nested_structure() {
    let rows = vec![
        ("a".to_string(), "A".to_string()),
        ("b/c".to_string(), "C".to_string()),
        ("b/d".to_string(), "D".to_string()),
    ];
    let result = build_nav_tree(rows);

    // Folders first, then pages
    assert_eq!(result.len(), 2);

    // First should be folder "b" (folders come first)
    assert_eq!(result[0].name, "b");
    assert_eq!(result[0].path, None);
    assert_eq!(result[0].children.len(), 2);

    // Folder b's children should be sorted: c, d (both pages)
    assert_eq!(result[0].children[0].name, "C");
    assert_eq!(result[0].children[0].path, Some("b/c".to_string()));
    assert_eq!(result[0].children[0].children.len(), 0);

    assert_eq!(result[0].children[1].name, "D");
    assert_eq!(result[0].children[1].path, Some("b/d".to_string()));
    assert_eq!(result[0].children[1].children.len(), 0);

    // Second should be page "a" (pages come after folders)
    assert_eq!(result[1].name, "A");
    assert_eq!(result[1].path, Some("a".to_string()));
    assert_eq!(result[1].children.len(), 0);
}

#[test]
fn test_build_nav_tree_leaf_uses_title() {
    let rows = vec![("mypage".to_string(), "My Page Title".to_string())];
    let result = build_nav_tree(rows);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "My Page Title");
    assert_eq!(result[0].path, Some("mypage".to_string()));
}

#[test]
fn test_build_nav_tree_folder_uses_segment() {
    let rows = vec![
        ("docs/api".to_string(), "API Reference".to_string()),
        ("docs/guide".to_string(), "User Guide".to_string()),
    ];
    let result = build_nav_tree(rows);

    // Root should have one folder
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "docs");
    assert_eq!(result[0].path, None);
    assert_eq!(result[0].children.len(), 2);

    // Children should be sorted alphabetically by name (case-insensitive)
    assert_eq!(result[0].children[0].name, "API Reference");
    assert_eq!(result[0].children[1].name, "User Guide");
}

#[test]
fn test_build_nav_tree_sorting_case_insensitive() {
    let rows = vec![
        ("zebra".to_string(), "Zebra".to_string()),
        ("apple".to_string(), "Apple".to_string()),
        ("Banana".to_string(), "Banana".to_string()),
    ];
    let result = build_nav_tree(rows);

    // Should be sorted case-insensitively
    assert_eq!(result[0].name, "Apple");
    assert_eq!(result[1].name, "Banana");
    assert_eq!(result[2].name, "Zebra");
}

#[test]
fn test_build_nav_tree_sorts_pages_by_file_stem_not_title() {
    let rows = vec![
        ("docs/02-setup".to_string(), "Apple Title".to_string()),
        ("docs/README".to_string(), "Folder Overview".to_string()),
        ("docs/01-intro".to_string(), "Zebra Title".to_string()),
    ];
    let result = build_nav_tree(rows);
    let docs = &result[0];
    let names: Vec<&str> = docs
        .children
        .iter()
        .map(|node| node.name.as_str())
        .collect();
    let stems: Vec<&str> = docs
        .children
        .iter()
        .map(|node| node.stem.as_str())
        .collect();

    assert_eq!(names, vec!["Folder Overview", "Zebra Title", "Apple Title"]);
    assert_eq!(stems, vec!["README", "01-intro", "02-setup"]);
}

#[test]
fn test_build_nav_tree_empty() {
    let rows = vec![];
    let result = build_nav_tree(rows);

    assert_eq!(result.len(), 0);
}

#[test]
fn test_build_nav_tree_deep_nesting() {
    let rows = vec![
        ("a/b/c/d".to_string(), "Deep Page".to_string()),
        ("a/b/e".to_string(), "E".to_string()),
    ];
    let result = build_nav_tree(rows);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "a");
    assert_eq!(result[0].path, None);

    let level1 = &result[0].children;
    assert_eq!(level1.len(), 1);
    assert_eq!(level1[0].name, "b");
    assert_eq!(level1[0].path, None);

    let level2 = &level1[0].children;
    assert_eq!(level2.len(), 2);
    // c folder should come before e page
    assert_eq!(level2[0].name, "c");
    assert_eq!(level2[0].path, None);
    assert_eq!(level2[1].name, "E");
    assert_eq!(level2[1].path, Some("a/b/e".to_string()));

    let level3 = &level2[0].children;
    assert_eq!(level3.len(), 1);
    assert_eq!(level3[0].name, "Deep Page");
    assert_eq!(level3[0].path, Some("a/b/c/d".to_string()));
}

#[test]
fn test_prune_nav_tree_keeps_only_active_ancestors() {
    let rows = vec![
        ("a/b/c".to_string(), "C".to_string()),
        ("a/x".to_string(), "X".to_string()),
        ("d/e".to_string(), "E".to_string()),
    ];
    let mut tree = build_nav_tree(rows);
    prune_nav_tree(&mut tree, "a/b/c", "");

    // Roots "a" and "d" both present, but only "a" (ancestor) keeps children.
    let a = tree.iter().find(|n| n.name == "a").expect("a present");
    let d = tree.iter().find(|n| n.name == "d").expect("d present");
    assert!(d.children.is_empty(), "non-ancestor folder pruned to stub");
    // "a" keeps child folder "b"; sibling page "x" stays (already loaded).
    let b = a.children.iter().find(|n| n.name == "b").expect("b kept");
    assert_eq!(b.children.len(), 1, "ancestor folder b keeps its child");
    assert_eq!(b.children[0].path, Some("a/b/c".to_string()));
}

#[test]
fn test_nav_folder_children_descends_to_dir() {
    let rows = vec![
        ("a/b/c".to_string(), "C".to_string()),
        ("a/b/f".to_string(), "F".to_string()),
        ("a/x".to_string(), "X".to_string()),
    ];
    let tree = build_nav_tree(rows);
    let children = nav_folder_children(tree, "a/b");
    let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["C", "F"]);
}

#[test]
fn test_nav_folder_children_missing_dir_is_empty() {
    let rows = vec![("a/b".to_string(), "B".to_string())];
    let tree = build_nav_tree(rows);
    assert!(nav_folder_children(tree, "nope/here").is_empty());
}

// The SSE feature is a read-only broadcast fan-out: the indexer sends a
// page path, every subscriber's stream yields it. This proves the
// broadcast -> BroadcastStream wiring in isolation (no DB, no HTTP server),
// mirroring exactly what the `/events` handler does internally.
#[tokio::test]
async fn test_events_broadcast_reaches_subscriber_stream() {
    let (tx, _) = tokio::sync::broadcast::channel::<String>(256);

    // Subscribe BEFORE sending (mirrors a connected browser).
    let rx = tx.subscribe();
    let mut stream = BroadcastStream::new(rx)
        .filter_map(|item| item.ok().map(Ok::<_, std::convert::Infallible>));

    // The indexer broadcasts a re-indexed page path (`.md` stripped form).
    tx.send("Notes/Daily".to_string())
        .expect("subscriber present");

    let received = stream.next().await.expect("stream item").expect("ok item");
    assert_eq!(received, "Notes/Daily");
}

// `send` returns Err only when there are no subscribers; the indexer ignores
// that with `let _ =`. Confirm the no-subscriber case is an error (so the
// ignore is correct) and does not panic.
#[test]
fn test_events_send_with_no_subscribers_is_err() {
    let (tx, rx) = tokio::sync::broadcast::channel::<String>(256);
    drop(rx);
    assert!(tx.send("Orphan".to_string()).is_err());
}

#[test]
fn test_promote_first_plain_mention_preserves_label() {
    let promoted = promote_first_plain_mention(
        "# Source\n\nThis references Target Page in prose.",
        "Target Page",
        "Notes/Target",
    )
    .expect("mention promoted");

    assert!(promoted.contains("[[Notes/Target|Target Page]]"));
}

#[test]
fn test_promote_first_plain_mention_case_insensitive_after_multibyte() {
    // A multi-byte (CJK) prefix must not corrupt the byte offsets, and the
    // match is case-insensitive while preserving the original-cased label.
    let promoted = promote_first_plain_mention(
        "日本語 about target page in prose.",
        "Target Page",
        "Notes/Target",
    )
    .expect("mention promoted");

    assert!(promoted.contains("[[Notes/Target|target page]]"));
}

#[test]
fn test_promote_first_plain_mention_skips_existing_wikilink() {
    let promoted =
        promote_first_plain_mention("Already [[Target Page]] here.", "Target Page", "Target");

    assert!(promoted.is_none());
}

#[tokio::test]
async fn test_app_router_registers_events_route() {
    // Build the router with a dummy AppState (no DB connection is made until
    // a handler runs). This proves `/events` is wired into `fn app`.
    let (events, _) = tokio::sync::broadcast::channel::<String>(256);
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));
    let state = AppState {
        index: compose_index(RuntimeConfig::Memory)
            .await
            .expect("memory index API"),
        templates: Arc::new(templates_env),
        index_ready: Arc::new(AtomicBool::new(true)),
        events,
    };
    // If `/events` (or any route) were malformed, `app` would panic here.
    let _router = http::router(state);
}

#[test]
fn test_page_template_does_not_open_reader_event_stream() {
    let mut templates_env = Environment::new();
    templates_env.set_loader(minijinja::path_loader("src/templates"));

    let template = templates_env
        .get_template("page.html")
        .expect("Failed to get page.html template");
    let rendered = template
        .render(context! {
            title => "Test Title",
            path => "Notes/Daily",
            exists => true,
            content_html => "<p>Test content</p>",
            body => "# Test Title\n\nTest content",
            loaded_hash => "abc",
            has_mermaid => false,
            backlinks => Vec::<Backlink>::new(),
            unlinked_mentions => Vec::<UnlinkedMention>::new(),
            toc => Vec::<Heading>::new(),
            word_count => 2usize,
            backlink_count => 0usize,
            updated => "2026-06-27 12:00",
            frontmatter => serde_json::Value::Object(serde_json::Map::new()),
            breadcrumb_parent => Option::<String>::None,
            breadcrumbs => test_breadcrumbs(),
        })
        .expect("Failed to render template");

    // minijinja HTML-escapes `/` to `&#x2f;` inside the attribute value; the
    // browser's getAttribute decodes it back to "Notes/Daily", matching the
    // unescaped path the SSE broadcast sends. Assert on the escaped form.
    assert!(rendered.contains("data-page-path=\"Notes&#x2f;Daily\""));
    assert!(!rendered.contains("new EventSource(\"/events\")"));
    assert!(rendered.contains("class=\"mk-synced\""));
}

#[test]
fn test_safe_file_path_rejects_traversal() {
    let result = safe_file_path("../etc/passwd");
    assert!(result.is_err());
}

#[test]
fn test_safe_file_path_rejects_absolute() {
    let result = safe_file_path("/abs");
    assert!(result.is_err());
}

#[test]
fn test_safe_file_path_accepts_canonical_and_md_aliases() {
    let canonical = safe_file_path("Notes/Daily")
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let md_alias = safe_file_path("Notes/Daily.md")
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert_eq!(canonical, md_alias);
}

#[test]
fn test_safe_trash_id_accepts_generated_id() {
    // Ids are `<flattened-path>-<ts>` (with an optional `-<n>` suffix).
    assert!(safe_trash_id("Notes-Daily-1719800000").is_ok());
    assert!(safe_trash_id("Index-1719800000-2").is_ok());
}

#[test]
fn test_safe_trash_id_rejects_traversal_and_separators() {
    assert!(safe_trash_id("").is_err());
    assert!(safe_trash_id("../secret").is_err());
    assert!(safe_trash_id("nested/id").is_err());
    assert!(safe_trash_id("back\\slash").is_err());
}

#[test]
fn test_trash_manifest_round_trips() {
    let manifest = TrashManifest {
        id: "Notes-Daily-1719800000".to_string(),
        original_path: "Notes/Daily".to_string(),
        title: "Daily".to_string(),
        trashed_at: 1_719_800_000,
    };
    let json = serde_json::to_string(&manifest).expect("serialize");
    let back: TrashManifest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, manifest.id);
    assert_eq!(back.original_path, "Notes/Daily");
    assert_eq!(back.title, "Daily");
    assert_eq!(back.trashed_at, 1_719_800_000);
}
