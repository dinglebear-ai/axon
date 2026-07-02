use super::*;
use serde_json::json;

#[test]
fn maps_subreddit_listing_to_dump_items() {
    let listing = json!({
        "kind": "Listing",
        "data": {
            "after": "t3_next",
            "children": [
                {
                    "kind": "t3",
                    "data": {
                        "title": "Rust chunking",
                        "selftext": "Post body",
                        "permalink": "/r/rust/comments/abc/rust_chunking/",
                        "author": "alice",
                        "score": 42,
                        "subreddit": "rust",
                        "domain": "self.rust",
                        "num_comments": 3,
                        "upvote_ratio": 0.95,
                        "is_video": false,
                        "distinguished": "moderator",
                        "gilded": 1,
                        "link_flair_text": "discussion",
                        "created_utc": 1_767_225_600.0
                    }
                }
            ]
        }
    });

    let items = map_subreddit_listing(&listing);
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.title.as_deref(), Some("Rust chunking"));
    assert_eq!(item.selftext.as_deref(), Some("Post body"));
    assert_eq!(
        item.permalink.as_deref(),
        Some("/r/rust/comments/abc/rust_chunking/")
    );
    assert_eq!(item.author.as_deref(), Some("alice"));
    assert_eq!(item.score, Some(42));
    assert_eq!(item.subreddit.as_deref(), Some("rust"));
    assert_eq!(item.domain.as_deref(), Some("self.rust"));
    assert_eq!(item.num_comments, Some(3));
    assert_eq!(item.upvote_ratio, Some(0.95));
    assert_eq!(item.is_video, Some(false));
    assert_eq!(item.distinguished.as_deref(), Some("moderator"));
    assert_eq!(item.gilded, Some(1));
    assert_eq!(item.link_flair_text.as_deref(), Some("discussion"));
    // Float epoch truncated to whole seconds.
    assert_eq!(item.created_utc, Some(1_767_225_600));
    // Listings carry no comment tree.
    assert!(item.comments.is_empty());
}

#[test]
fn empty_string_optional_fields_become_none() {
    let listing = json!({
        "data": {
            "children": [
                {
                    "kind": "t3",
                    "data": {
                        "title": "Only a title",
                        "selftext": "",
                        "permalink": "/r/rust/comments/x/only/",
                        "author": "",
                        "score": 5
                    }
                }
            ]
        }
    });

    let items = map_subreddit_listing(&listing);
    let item = &items[0];
    // Empty selftext/author collapse to None (adapter treats them as absent).
    assert_eq!(item.selftext, None);
    assert_eq!(item.author, None);
    // Absent numeric/bool fields are None, not defaulted.
    assert_eq!(item.num_comments, None);
    assert_eq!(item.is_video, None);
    assert_eq!(item.created_utc, None);
}

#[test]
fn listing_without_children_maps_to_empty() {
    assert!(map_subreddit_listing(&json!({})).is_empty());
    assert!(map_subreddit_listing(&json!({"data": {}})).is_empty());
}

#[test]
fn maps_thread_with_flattened_comments() {
    let thread = json!([
        {
            "kind": "Listing",
            "data": {
                "children": [
                    {
                        "kind": "t3",
                        "data": {
                            "title": "Thread title",
                            "selftext": "Thread body",
                            "permalink": "/r/rust/comments/abc/thread_title/",
                            "author": "op",
                            "score": 100,
                            "subreddit": "rust"
                        }
                    }
                ]
            }
        },
        {
            "kind": "Listing",
            "data": {
                "children": [
                    {
                        "kind": "t1",
                        "data": {
                            "body": "Top level comment",
                            "score": 10,
                            "replies": {
                                "kind": "Listing",
                                "data": {
                                    "children": [
                                        {
                                            "kind": "t1",
                                            "data": {
                                                "body": "Nested reply",
                                                "score": 4,
                                                "replies": ""
                                            }
                                        }
                                    ]
                                }
                            }
                        }
                    }
                ]
            }
        }
    ]);

    let item = map_thread(&thread).expect("thread should map to a post item");
    assert_eq!(item.title.as_deref(), Some("Thread title"));
    assert_eq!(item.selftext.as_deref(), Some("Thread body"));
    assert_eq!(item.comments.len(), 2);
    assert_eq!(item.comments[0].body, "Top level comment");
    assert_eq!(item.comments[0].parent_text, None);
    // Nested reply carries the parent comment body for threading context.
    assert_eq!(item.comments[1].body, "Nested reply");
    assert_eq!(
        item.comments[1].parent_text.as_deref(),
        Some("Top level comment")
    );
}

#[test]
fn thread_drops_low_score_and_deleted_comments() {
    let thread = json!([
        {
            "data": {
                "children": [
                    { "kind": "t3", "data": { "title": "T", "permalink": "/r/x/comments/y/t/" } }
                ]
            }
        },
        {
            "data": {
                "children": [
                    { "kind": "t1", "data": { "body": "low score", "score": 0 } },
                    { "kind": "t1", "data": { "body": "[deleted]", "score": 50 } },
                    { "kind": "t1", "data": { "body": "[removed]", "score": 50 } },
                    { "kind": "t1", "data": { "body": "", "score": 50 } },
                    { "kind": "t1", "data": { "body": "kept", "score": 3 } },
                    { "kind": "more", "data": { "body": "not a comment", "score": 99 } }
                ]
            }
        }
    ]);

    let item = map_thread(&thread).expect("thread maps");
    // Only the single positive-score, non-deleted, `t1` comment survives.
    assert_eq!(item.comments.len(), 1);
    assert_eq!(item.comments[0].body, "kept");
}

#[test]
fn thread_without_post_data_is_none() {
    // Missing t3 post data => no item.
    let thread = json!([{ "data": { "children": [] } }, { "data": { "children": [] } }]);
    assert!(map_thread(&thread).is_none());
    assert!(map_thread(&json!([])).is_none());
}

#[test]
fn thread_without_comment_listing_maps_post_only() {
    let thread = json!([
        {
            "data": {
                "children": [
                    { "kind": "t3", "data": { "title": "Solo", "permalink": "/r/x/comments/z/solo/" } }
                ]
            }
        }
    ]);
    let item = map_thread(&thread).expect("post-only thread maps");
    assert_eq!(item.title.as_deref(), Some("Solo"));
    assert!(item.comments.is_empty());
}

#[test]
fn serialized_dump_deserializes_back_into_dump_shape() {
    // The serialized dump MUST round-trip through a struct with the exact field
    // names the adapter's `RedditDumpItem`/`RedditDumpComment` deserialize
    // targets use — this guards against a field-name drift between the acquire
    // mapping and the adapter parser.
    #[derive(serde::Deserialize)]
    struct MirrorItem {
        title: Option<String>,
        #[serde(default)]
        selftext: Option<String>,
        permalink: Option<String>,
        author: Option<String>,
        score: Option<i64>,
        subreddit: Option<String>,
        domain: Option<String>,
        num_comments: Option<u64>,
        upvote_ratio: Option<f64>,
        is_video: Option<bool>,
        distinguished: Option<String>,
        gilded: Option<u64>,
        link_flair_text: Option<String>,
        created_utc: Option<u64>,
        #[serde(default)]
        comments: Vec<MirrorComment>,
    }
    #[derive(serde::Deserialize)]
    struct MirrorComment {
        body: String,
        #[serde(default)]
        parent_text: Option<String>,
    }

    let thread = json!([
        {
            "data": {
                "children": [
                    {
                        "kind": "t3",
                        "data": {
                            "title": "Roundtrip",
                            "permalink": "/r/rust/comments/rt/roundtrip/",
                            "score": 7,
                            "created_utc": 1_700_000_000.0
                        }
                    }
                ]
            }
        },
        {
            "data": {
                "children": [
                    { "kind": "t1", "data": { "body": "hi", "score": 2 } }
                ]
            }
        }
    ]);
    let item = map_thread(&thread).expect("maps");
    let bytes = serde_json::to_vec(&vec![item]).expect("serialize dump");

    let parsed: Vec<MirrorItem> =
        serde_json::from_slice(&bytes).expect("dump must parse into adapter dump shape");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].title.as_deref(), Some("Roundtrip"));
    assert_eq!(parsed[0].score, Some(7));
    assert_eq!(parsed[0].created_utc, Some(1_700_000_000));
    assert_eq!(parsed[0].comments.len(), 1);
    assert_eq!(parsed[0].comments[0].body, "hi");
    // Silence dead-field warnings on the mirror structs — the deserialize itself
    // is the assertion.
    let _ = (
        &parsed[0].selftext,
        &parsed[0].permalink,
        &parsed[0].author,
        &parsed[0].subreddit,
        &parsed[0].domain,
        &parsed[0].num_comments,
        &parsed[0].upvote_ratio,
        &parsed[0].is_video,
        &parsed[0].distinguished,
        &parsed[0].gilded,
        &parsed[0].link_flair_text,
        &parsed[0].comments[0].parent_text,
    );
}
