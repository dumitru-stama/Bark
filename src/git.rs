//! Git repository status helpers

use std::path::Path;
use std::process::Command;

/// Git repository status
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    /// Current branch name (or HEAD if detached)
    pub branch: Option<String>,
    /// Has uncommitted changes
    pub dirty: bool,
    /// Has staged changes
    pub staged: bool,
    /// Has untracked files
    pub untracked: bool,
    /// Commits ahead of remote
    pub ahead: u32,
    /// Commits behind remote
    pub behind: u32,
}

impl GitStatus {
    /// Format for display in status bar: [branch *+?] or [branch ↑2↓3]
    pub fn format(&self) -> String {
        let Some(ref branch) = self.branch else {
            return String::new();
        };

        let mut result = format!("[{}", branch);

        // Show ahead/behind
        if self.ahead > 0 {
            result.push_str(&format!("↑{}", self.ahead));
        }
        if self.behind > 0 {
            result.push_str(&format!("↓{}", self.behind));
        }

        // Show status indicators
        let mut indicators = String::new();
        if self.staged {
            indicators.push('+');
        }
        if self.dirty {
            indicators.push('*');
        }
        if self.untracked {
            indicators.push('?');
        }

        if !indicators.is_empty() {
            result.push(' ');
            result.push_str(&indicators);
        }

        result.push(']');
        result
    }
}

/// Get git status for a directory
pub fn get_git_status(path: &Path) -> Option<GitStatus> {
    // Check if we're in a git repo by getting the branch
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .ok()?;

    if !branch_output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    let branch = if branch == "HEAD" {
        // Detached HEAD - get short commit hash
        let hash_output = Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(path)
            .output()
            .ok()?;
        if hash_output.status.success() {
            format!(":{}", String::from_utf8_lossy(&hash_output.stdout).trim())
        } else {
            "HEAD".to_string()
        }
    } else {
        branch
    };

    // Get status (porcelain for easy parsing)
    let status_output = Command::new("git")
        .args(["status", "--porcelain", "--branch"])
        .current_dir(path)
        .output()
        .ok()?;

    let status_str = String::from_utf8_lossy(&status_output.stdout);
    let mut dirty = false;
    let mut staged = false;
    let mut untracked = false;
    let mut ahead = 0u32;
    let mut behind = 0u32;

    for line in status_str.lines() {
        if line.starts_with("##") {
            // Branch line: ## main...origin/main [ahead 1, behind 2]
            if let Some(bracket_start) = line.find('[') {
                let info = &line[bracket_start..];
                if let Some(a) = info.find("ahead ") {
                    let rest = &info[a + 6..];
                    if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                        ahead = rest[..end].parse().unwrap_or(0);
                    } else {
                        ahead = rest.trim_end_matches(']').parse().unwrap_or(0);
                    }
                }
                if let Some(b) = info.find("behind ") {
                    let rest = &info[b + 7..];
                    if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                        behind = rest[..end].parse().unwrap_or(0);
                    } else {
                        behind = rest.trim_end_matches(']').parse().unwrap_or(0);
                    }
                }
            }
        } else if line.len() >= 2 {
            let index = line.chars().next().unwrap_or(' ');
            let worktree = line.chars().nth(1).unwrap_or(' ');

            // Check index (staged) status
            if index != ' ' && index != '?' {
                staged = true;
            }

            // Check worktree (dirty) status
            if worktree != ' ' && worktree != '?' {
                dirty = true;
            }

            // Check for untracked
            if index == '?' {
                untracked = true;
            }
        }
    }

    Some(GitStatus {
        branch: Some(branch),
        dirty,
        staged,
        untracked,
        ahead,
        behind,
    })
}
