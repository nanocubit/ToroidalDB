//! # Git VCS Integration для GuiTor
//! 
//! Интеграция с Git для версионирования SQL схем

use anyhow::{Context, Result};
use git2::{Repository, CommitOptions, Signature, IndexAddOption};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Git Version Control Manager
pub struct GitVcs {
    repo_path: PathBuf,
    repository: Option<Repository>,
}

/// Git commit информация
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    pub id: String,
    pub message: String,
    pub author: String,
    pub timestamp: u64,
    pub files_changed: Vec<String>,
}

/// Git branch информация
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBranch {
    pub name: String,
    pub is_current: bool,
    pub last_commit: Option<GitCommit>,
}

impl GitVcs {
    pub fn new(repo_path: &str) -> Self {
        Self {
            repo_path: PathBuf::from(repo_path),
            repository: None,
        }
    }

    /// Инициализирует или открывает репозиторий
    pub fn init_or_open(&mut self) -> Result<()> {
        if self.repo_path.exists() {
            self.repository = Some(Repository::open(&self.repo_path)?);
        } else {
            std::fs::create_dir_all(&self.repo_path)?;
            self.repository = Some(Repository::init(&self.repo_path)?);
        }
        Ok(())
    }

    /// Коммитит SQL файл
    pub fn commit_sql(&self, file_path: &str, message: &str) -> Result<String> {
        let repo = self.repository.as_ref()
            .context("Repository not initialized")?;

        // Добавляем файл в индекс
        let mut index = repo.index()?;
        index.add_path(Path::new(file_path), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        // Создаём коммит
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        
        let signature = Signature::now("GuiTor", "guitor@example.com")?;
        
        let commit_id = if let Ok(head) = repo.head() {
            let parent = repo.find_commit(head.target().unwrap())?;
            repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&parent])?
        } else {
            repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?
        };

        Ok(commit_id.to_string())
    }

    /// Получает историю коммитов
    pub fn get_history(&self, limit: usize) -> Result<Vec<GitCommit>> {
        let repo = self.repository.as_ref()
            .context("Repository not initialized")?;

        let mut commits = Vec::new();
        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;

        for oid in revwalk.take(limit) {
            let oid = oid?;
            let commit = repo.find_commit(oid)?;
            
            commits.push(GitCommit {
                id: oid.to_string(),
                message: commit.message().unwrap_or("").to_string(),
                author: commit.author().name().unwrap_or("").to_string(),
                timestamp: commit.time().seconds() as u64,
                files_changed: vec![], // Можно реализовать подсчёт файлов
            });
        }

        Ok(commits)
    }

    /// Создаёт новую ветку
    pub fn create_branch(&self, branch_name: &str, from: &str) -> Result<()> {
        let repo = self.repository.as_ref()
            .context("Repository not initialized")?;

        let from_commit = if from == "HEAD" {
            repo.head()?.peel_to_commit()?
        } else {
            repo.find_branch(from, git2::BranchType::Local)?
                .into_reference()
                .peel_to_commit()?
        };

        repo.branch(branch_name, &from_commit, false)?;
        Ok(())
    }

    /// Переключается на ветку
    pub fn checkout_branch(&self, branch_name: &str) -> Result<()> {
        let repo = self.repository.as_ref()
            .context("Repository not initialized")?;

        let branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
        let object = branch.into_reference().peel_to_tree()?;
        
        repo.checkout_tree(&object, None)?;
        repo.set_head(branch.get().name().unwrap())?;

        Ok(())
    }

    /// Получает список веток
    pub fn list_branches(&self) -> Result<Vec<GitBranch>> {
        let repo = self.repository.as_ref()
            .context("Repository not initialized")?;

        let mut branches = Vec::new();
        let current_head = repo.head().ok().map(|h| h.name().unwrap().to_string());

        for branch in repo.branches(Some(git2::BranchType::Local))? {
            let (branch, _) = branch?;
            let name = branch.name()?.unwrap().to_string();
            
            branches.push(GitBranch {
                name,
                is_current: current_head.as_ref().map(|h| h.contains(&branch.name().unwrap().unwrap())).unwrap_or(false),
                last_commit: None,
            });
        }

        Ok(branches)
    }

    /// Сравнивает две версии
    pub fn diff(&self, from: &str, to: &str) -> Result<String> {
        let repo = self.repository.as_ref()
            .context("Repository not initialized")?;

        let from_commit = self.resolve_commit(from)?;
        let to_commit = self.resolve_commit(to)?;

        let from_tree = from_commit.tree()?;
        let to_tree = to_commit.tree()?;

        let diff = repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)?;
        
        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
            let sign = match line.origin() {
                '+' | '\n' => "",
                _ => " ",
            };
            diff_text.push_str(&format!("{}{}", sign, std::str::from_utf8(line.content()).unwrap_or("")));
            true
        })?;

        Ok(diff_text)
    }

    fn resolve_commit(&self, reference: &str) -> Result<git2::Commit> {
        let repo = self.repository.as_ref()
            .context("Repository not initialized")?;

        if reference == "HEAD" {
            Ok(repo.head()?.peel_to_commit()?)
        } else {
            let object = repo.revparse_single(reference)?;
            Ok(object.peel_to_commit()?)
        }
    }

    /// Экспортирует схему в Git
    pub fn export_schema_to_git(&self, schema_name: &str, sql_content: &str) -> Result<String> {
        let file_path = format!("schemas/{}/schema.sql", schema_name);
        
        // Создаём директорию
        let full_path = self.repo_path.join(&file_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Записываем файл
        std::fs::write(&full_path, sql_content)?;

        // Коммитим
        self.commit_sql(&file_path, &format!("Export schema {}", schema_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_repo() {
        let dir = tempdir().unwrap();
        let mut vcs = GitVcs::new(dir.path().to_str().unwrap());
        
        assert!(vcs.init_or_open().is_ok());
        assert!(vcs.repository.is_some());
    }

    #[test]
    fn test_commit_sql() {
        let dir = tempdir().unwrap();
        let mut vcs = GitVcs::new(dir.path().to_str().unwrap());
        vcs.init_or_open().unwrap();

        let sql = "CREATE TABLE test (id INT);";
        std::fs::write(dir.path().join("test.sql"), sql).unwrap();

        let commit_id = vcs.commit_sql("test.sql", "Initial commit");
        assert!(commit_id.is_ok());
    }
}
