use anyhow::Result;

use super::super::domains::update;

pub(super) fn update_cmd(repo: Option<String>) -> Result<()> {
    let repo = repo.unwrap_or_else(|| "nimeview/nimesvc".to_string());
    update::update_to_latest(&repo)
}
