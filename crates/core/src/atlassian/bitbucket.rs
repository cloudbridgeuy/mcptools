//! Transformation functions for Bitbucket API responses

use serde::{Deserialize, Serialize};

// =============================================================================
// API Response Types (Deserialization)
// =============================================================================

/// Pull request response from Bitbucket API
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketPRResponse {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub state: String, // OPEN, MERGED, DECLINED, SUPERSEDED
    pub author: BitbucketUser,
    pub source: BitbucketRef,
    pub destination: BitbucketRef,
    pub created_on: String,
    pub updated_on: String,
    #[serde(default)]
    pub reviewers: Vec<BitbucketUser>,
    #[serde(default)]
    pub participants: Vec<BitbucketParticipant>,
    pub links: BitbucketPRLinks,
}

/// Bitbucket user information
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketUser {
    pub display_name: String,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

/// Reference to a branch in a repository
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketRef {
    pub branch: BitbucketBranch,
    pub repository: BitbucketRepository,
    #[serde(default)]
    pub commit: Option<BitbucketCommit>,
}

/// Commit reference with hash
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketCommit {
    pub hash: String,
}

/// Branch information
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketBranch {
    pub name: String,
}

/// Repository information
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketRepository {
    pub full_name: String,
    pub name: String,
}

/// Participant in a pull request (reviewer with status)
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketParticipant {
    pub user: BitbucketUser,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub state: Option<String>, // approved, changes_requested, null
}

/// Links in PR response
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketPRLinks {
    #[serde(rename = "self")]
    pub self_link: BitbucketLink,
    pub html: BitbucketLink,
    pub diff: BitbucketLink,
}

/// A single link
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketLink {
    pub href: String,
}

/// Paginated comments response from Bitbucket API
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketCommentsResponse {
    pub values: Vec<BitbucketComment>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub size: Option<u32>,
}

/// A comment on a pull request
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketComment {
    pub id: u64,
    pub user: BitbucketUser,
    pub content: BitbucketContent,
    pub created_on: String,
    pub updated_on: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub inline: Option<BitbucketInlineComment>,
    #[serde(default)]
    pub parent: Option<BitbucketCommentParent>,
}

/// Content of a comment (supports multiple formats)
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketContent {
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub markup: Option<String>,
    #[serde(default)]
    pub html: Option<String>,
}

/// Inline comment location
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketInlineComment {
    pub path: String,
    #[serde(default)]
    pub from: Option<u32>,
    #[serde(default)]
    pub to: Option<u32>,
}

/// Parent comment reference
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketCommentParent {
    pub id: u64,
}

/// Paginated diffstat response from Bitbucket API
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketDiffstatResponse {
    pub values: Vec<BitbucketDiffstat>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub size: Option<u32>,
}

/// A single file's diff statistics
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketDiffstat {
    pub status: String, // added, removed, modified, renamed
    #[serde(default)]
    pub lines_added: u32,
    #[serde(default)]
    pub lines_removed: u32,
    #[serde(default)]
    pub old: Option<BitbucketCommitFile>,
    #[serde(default)]
    pub new: Option<BitbucketCommitFile>,
}

/// A file reference in a commit
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketCommitFile {
    pub path: String,
    #[serde(default)]
    pub escaped_path: Option<String>,
}

// =============================================================================
// Output Domain Types (Clean models for display)
// =============================================================================

/// Output structure for PR read command
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PROutput {
    pub id: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub author: String,
    pub source_branch: String,
    pub destination_branch: String,
    pub source_repo: String,
    pub destination_repo: String,
    pub source_commit: Option<String>,
    pub destination_commit: Option<String>,
    pub created_on: String,
    pub updated_on: String,
    pub reviewers: Vec<String>,
    pub approvals: Vec<String>,
    pub html_link: String,
    pub diffstat: DiffstatOutput,
    pub diff_content: Option<String>,
    pub comments: Vec<CommentOutput>,
}

/// Output structure for a single comment
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct CommentOutput {
    pub id: u64,
    pub author: String,
    pub content: String,
    pub created_on: String,
    pub is_inline: bool,
    pub inline_path: Option<String>,
    pub inline_line: Option<u32>,
}

/// Output structure for diff statistics
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct DiffstatOutput {
    pub files: Vec<FileStatOutput>,
    pub total_files: usize,
    pub total_insertions: u32,
    pub total_deletions: u32,
}

/// Output structure for a single file's diff statistics
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct FileStatOutput {
    pub path: String,
    pub old_path: Option<String>, // For renames
    pub status: String,
    pub lines_added: u32,
    pub lines_removed: u32,
}

// =============================================================================
// Pure Transformation Functions
// =============================================================================

/// Transform Bitbucket PR API response to output domain model
///
/// Combines PR details, comments, and diffstats into a single output structure.
///
/// # Arguments
/// * `pr` - The raw PR response from Bitbucket API
/// * `comments` - The list of comments (already fetched and combined)
/// * `diffstats` - The list of diffstat entries (already fetched and combined)
///
/// # Returns
/// * `PROutput` - Cleaned and transformed PR with all details
pub fn transform_pr_response(
    pr: BitbucketPRResponse,
    comments: Vec<BitbucketComment>,
    diffstats: Vec<BitbucketDiffstat>,
    diff_content: Option<String>,
) -> PROutput {
    // Extract approvals from participants
    let approvals: Vec<String> = pr
        .participants
        .iter()
        .filter(|p| p.approved)
        .map(|p| p.user.display_name.clone())
        .collect();

    // Transform comments, filtering deleted ones
    let comment_outputs: Vec<CommentOutput> = comments
        .into_iter()
        .filter(|c| !c.deleted)
        .map(transform_comment)
        .collect();

    // Transform diffstats
    let diffstat_output = transform_diffstat_response(diffstats);

    PROutput {
        id: pr.id,
        title: pr.title,
        description: pr.description.filter(|d| !d.is_empty()),
        state: pr.state,
        author: pr.author.display_name,
        source_branch: pr.source.branch.name.clone(),
        destination_branch: pr.destination.branch.name.clone(),
        source_repo: pr.source.repository.full_name.clone(),
        destination_repo: pr.destination.repository.full_name.clone(),
        source_commit: pr.source.commit.map(|c| c.hash),
        destination_commit: pr.destination.commit.map(|c| c.hash),
        created_on: pr.created_on,
        updated_on: pr.updated_on,
        reviewers: pr
            .reviewers
            .iter()
            .map(|r| r.display_name.clone())
            .collect(),
        approvals,
        html_link: pr.links.html.href,
        diffstat: diffstat_output,
        diff_content,
        comments: comment_outputs,
    }
}

/// Transform diffstat entries to output domain model
///
/// # Arguments
/// * `diffstats` - The list of diffstat entries from Bitbucket API
///
/// # Returns
/// * `DiffstatOutput` - Cleaned and summarized diffstat with totals
pub fn transform_diffstat_response(diffstats: Vec<BitbucketDiffstat>) -> DiffstatOutput {
    let mut total_insertions: u32 = 0;
    let mut total_deletions: u32 = 0;

    let files: Vec<FileStatOutput> = diffstats
        .into_iter()
        .map(|d| {
            total_insertions += d.lines_added;
            total_deletions += d.lines_removed;

            // For renames, use new path as primary, old path as secondary
            let (path, old_path) = match (&d.new, &d.old) {
                (Some(new), Some(old)) if d.status == "renamed" => {
                    (new.path.clone(), Some(old.path.clone()))
                }
                (Some(new), _) => (new.path.clone(), None),
                (_, Some(old)) => (old.path.clone(), None),
                (None, None) => ("unknown".to_string(), None),
            };

            FileStatOutput {
                path,
                old_path,
                status: d.status,
                lines_added: d.lines_added,
                lines_removed: d.lines_removed,
            }
        })
        .collect();

    let total_files = files.len();

    DiffstatOutput {
        files,
        total_files,
        total_insertions,
        total_deletions,
    }
}

/// Transform a single comment to output format
fn transform_comment(comment: BitbucketComment) -> CommentOutput {
    // Prefer raw content, fall back to html (stripped)
    let content = comment
        .content
        .raw
        .or_else(|| comment.content.html.map(|h| strip_html(&h)))
        .unwrap_or_default();

    CommentOutput {
        id: comment.id,
        author: comment.user.display_name,
        content,
        created_on: comment.created_on,
        is_inline: comment.inline.is_some(),
        inline_path: comment.inline.as_ref().map(|i| i.path.clone()),
        inline_line: comment.inline.as_ref().and_then(|i| i.to.or(i.from)),
    }
}

/// Simple HTML tag stripping
fn strip_html(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_user(name: &str) -> BitbucketUser {
        BitbucketUser {
            display_name: name.to_string(),
            nickname: Some(name.to_lowercase()),
            account_id: Some(format!("{}-id", name.to_lowercase())),
        }
    }

    fn sample_ref(branch: &str, repo: &str, commit_hash: &str) -> BitbucketRef {
        BitbucketRef {
            branch: BitbucketBranch {
                name: branch.to_string(),
            },
            repository: BitbucketRepository {
                full_name: repo.to_string(),
                name: repo.split('/').next_back().unwrap_or(repo).to_string(),
            },
            commit: Some(BitbucketCommit {
                hash: commit_hash.to_string(),
            }),
        }
    }

    fn sample_pr() -> BitbucketPRResponse {
        BitbucketPRResponse {
            id: 123,
            title: "Add new feature".to_string(),
            description: Some("This PR adds a new feature".to_string()),
            state: "OPEN".to_string(),
            author: sample_user("John Doe"),
            source: sample_ref("feature-branch", "workspace/repo", "abc123"),
            destination: sample_ref("main", "workspace/repo", "def456"),
            created_on: "2025-01-15T10:30:00.000000+00:00".to_string(),
            updated_on: "2025-01-16T14:20:00.000000+00:00".to_string(),
            reviewers: vec![sample_user("Jane Smith")],
            participants: vec![BitbucketParticipant {
                user: sample_user("Jane Smith"),
                approved: true,
                state: Some("approved".to_string()),
            }],
            links: BitbucketPRLinks {
                self_link: BitbucketLink {
                    href: "https://api.bitbucket.org/2.0/repositories/workspace/repo/pullrequests/123".to_string(),
                },
                html: BitbucketLink {
                    href: "https://bitbucket.org/workspace/repo/pull-requests/123".to_string(),
                },
                diff: BitbucketLink {
                    href: "https://api.bitbucket.org/2.0/repositories/workspace/repo/pullrequests/123/diff".to_string(),
                },
            },
        }
    }

    fn sample_comment(id: u64, author: &str, content: &str) -> BitbucketComment {
        BitbucketComment {
            id,
            user: sample_user(author),
            content: BitbucketContent {
                raw: Some(content.to_string()),
                markup: Some("markdown".to_string()),
                html: None,
            },
            created_on: "2025-01-15T11:00:00.000000+00:00".to_string(),
            updated_on: "2025-01-15T11:00:00.000000+00:00".to_string(),
            deleted: false,
            inline: None,
            parent: None,
        }
    }

    fn sample_inline_comment(
        id: u64,
        author: &str,
        content: &str,
        path: &str,
        line: u32,
    ) -> BitbucketComment {
        BitbucketComment {
            id,
            user: sample_user(author),
            content: BitbucketContent {
                raw: Some(content.to_string()),
                markup: Some("markdown".to_string()),
                html: None,
            },
            created_on: "2025-01-15T12:00:00.000000+00:00".to_string(),
            updated_on: "2025-01-15T12:00:00.000000+00:00".to_string(),
            deleted: false,
            inline: Some(BitbucketInlineComment {
                path: path.to_string(),
                from: None,
                to: Some(line),
            }),
            parent: None,
        }
    }

    fn sample_diffstat(path: &str, status: &str, added: u32, removed: u32) -> BitbucketDiffstat {
        BitbucketDiffstat {
            status: status.to_string(),
            lines_added: added,
            lines_removed: removed,
            old: Some(BitbucketCommitFile {
                path: path.to_string(),
                escaped_path: None,
            }),
            new: Some(BitbucketCommitFile {
                path: path.to_string(),
                escaped_path: None,
            }),
        }
    }

    fn sample_renamed_diffstat(
        old_path: &str,
        new_path: &str,
        added: u32,
        removed: u32,
    ) -> BitbucketDiffstat {
        BitbucketDiffstat {
            status: "renamed".to_string(),
            lines_added: added,
            lines_removed: removed,
            old: Some(BitbucketCommitFile {
                path: old_path.to_string(),
                escaped_path: None,
            }),
            new: Some(BitbucketCommitFile {
                path: new_path.to_string(),
                escaped_path: None,
            }),
        }
    }

    #[test]
    fn test_transform_pr_response_basic() {
        let pr = sample_pr();
        let output = transform_pr_response(pr, vec![], vec![], None);

        assert_eq!(output.id, 123);
        assert_eq!(output.title, "Add new feature");
        assert_eq!(output.state, "OPEN");
        assert_eq!(output.author, "John Doe");
        assert_eq!(output.source_branch, "feature-branch");
        assert_eq!(output.destination_branch, "main");
        assert_eq!(output.source_commit, Some("abc123".to_string()));
        assert_eq!(output.destination_commit, Some("def456".to_string()));
        assert_eq!(output.reviewers, vec!["Jane Smith"]);
        assert_eq!(output.approvals, vec!["Jane Smith"]);
        assert!(output.comments.is_empty());
        assert!(output.diffstat.files.is_empty());
    }

    #[test]
    fn test_transform_pr_response_with_comments() {
        let pr = sample_pr();
        let comments = vec![
            sample_comment(1, "John Doe", "First comment"),
            sample_comment(2, "Jane Smith", "LGTM!"),
        ];
        let output = transform_pr_response(pr, comments, vec![], None);

        assert_eq!(output.comments.len(), 2);
        assert_eq!(output.comments[0].author, "John Doe");
        assert_eq!(output.comments[0].content, "First comment");
        assert_eq!(output.comments[1].author, "Jane Smith");
        assert_eq!(output.comments[1].content, "LGTM!");
    }

    #[test]
    fn test_transform_pr_response_filters_deleted_comments() {
        let pr = sample_pr();
        let mut deleted_comment = sample_comment(1, "John Doe", "This was deleted");
        deleted_comment.deleted = true;

        let comments = vec![
            deleted_comment,
            sample_comment(2, "Jane Smith", "This is visible"),
        ];
        let output = transform_pr_response(pr, comments, vec![], None);

        assert_eq!(output.comments.len(), 1);
        assert_eq!(output.comments[0].content, "This is visible");
    }

    #[test]
    fn test_transform_inline_comment() {
        let pr = sample_pr();
        let comments = vec![sample_inline_comment(
            1,
            "Jane Smith",
            "Fix this line",
            "src/main.rs",
            42,
        )];
        let output = transform_pr_response(pr, comments, vec![], None);

        assert_eq!(output.comments.len(), 1);
        assert!(output.comments[0].is_inline);
        assert_eq!(
            output.comments[0].inline_path,
            Some("src/main.rs".to_string())
        );
        assert_eq!(output.comments[0].inline_line, Some(42));
    }

    #[test]
    fn test_strip_html() {
        assert_eq!(strip_html("<p>Hello</p>"), "Hello");
        assert_eq!(
            strip_html("<b>Bold</b> and <i>italic</i>"),
            "Bold and italic"
        );
        assert_eq!(strip_html("No tags here"), "No tags here");
        assert_eq!(strip_html("<div><span>Nested</span></div>"), "Nested");
    }

    #[test]
    fn test_empty_description_becomes_none() {
        let mut pr = sample_pr();
        pr.description = Some("".to_string());
        let output = transform_pr_response(pr, vec![], vec![], None);
        assert_eq!(output.description, None);
    }

    #[test]
    fn test_no_approvals_when_none_approved() {
        let mut pr = sample_pr();
        pr.participants = vec![BitbucketParticipant {
            user: sample_user("Jane Smith"),
            approved: false,
            state: None,
        }];
        let output = transform_pr_response(pr, vec![], vec![], None);
        assert!(output.approvals.is_empty());
    }

    #[test]
    fn test_transform_diffstat_basic() {
        let diffstats = vec![
            sample_diffstat("src/main.rs", "modified", 10, 5),
            sample_diffstat("src/lib.rs", "added", 42, 0),
            sample_diffstat("tests/old.rs", "removed", 0, 20),
        ];
        let output = transform_diffstat_response(diffstats);

        assert_eq!(output.total_files, 3);
        assert_eq!(output.total_insertions, 52);
        assert_eq!(output.total_deletions, 25);
        assert_eq!(output.files[0].path, "src/main.rs");
        assert_eq!(output.files[0].status, "modified");
        assert_eq!(output.files[1].path, "src/lib.rs");
        assert_eq!(output.files[1].status, "added");
        assert_eq!(output.files[2].path, "tests/old.rs");
        assert_eq!(output.files[2].status, "removed");
    }

    #[test]
    fn test_transform_diffstat_renamed() {
        let diffstats = vec![sample_renamed_diffstat("old/file.rs", "new/file.rs", 5, 2)];
        let output = transform_diffstat_response(diffstats);

        assert_eq!(output.total_files, 1);
        assert_eq!(output.files[0].path, "new/file.rs");
        assert_eq!(output.files[0].old_path, Some("old/file.rs".to_string()));
        assert_eq!(output.files[0].status, "renamed");
    }

    #[test]
    fn test_transform_diffstat_empty() {
        let output = transform_diffstat_response(vec![]);

        assert_eq!(output.total_files, 0);
        assert_eq!(output.total_insertions, 0);
        assert_eq!(output.total_deletions, 0);
        assert!(output.files.is_empty());
    }

    #[test]
    fn test_transform_pr_response_with_diffstat() {
        let pr = sample_pr();
        let diffstats = vec![sample_diffstat("src/main.rs", "modified", 10, 5)];
        let output = transform_pr_response(pr, vec![], diffstats, None);

        assert_eq!(output.diffstat.total_files, 1);
        assert_eq!(output.diffstat.total_insertions, 10);
        assert_eq!(output.diffstat.total_deletions, 5);
    }
}
