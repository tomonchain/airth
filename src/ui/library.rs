//! The library's routes (decision 74): its teams, and installing one into a
//! business: its files into the business folder, its skills from the
//! catalog. Its connectors are the founder's to connect, each with its own
//! sign-in.

use super::*;
use crate::tools::catalog;

impl App {
    pub(super) fn library_routes(
        &self,
        _request: &mut Request,
        method: &Method,
        segments: &[&str],
        _query: &str,
    ) -> Result<Option<HttpResponse>> {
        Ok(Some(match (method, segments) {
            (Method::Get, ["realms", r, "library"]) => {
                let root = self.realm(r)?.root()?;
                let teams: Vec<Value> = crate::library::teams()
                    .iter()
                    .map(|t| crate::library::shown(t, Some(&root)))
                    .collect();
                json_response(200, json!({"teams": teams}))
            }
            (Method::Post, ["realms", r, "library", id, "install"]) => {
                let realm = self.realm(r)?;
                let root = realm.root()?;
                let team = crate::library::team(id)?;
                let written = crate::library::install(&root, &team)?;
                // Its skills, from the catalog as the Marketplace adds them.
                let (index, _) = catalog::index(&self.hub.home().dir().join("catalog.json"), false);
                for name in &team.skills {
                    if let Some(skill) = index.skills.iter().find(|s| &s.name == name) {
                        catalog::install_in_background(skill.clone(), realm.skills_dir());
                    }
                }
                json_response(
                    200,
                    json!({"written": written, "team": crate::library::shown(&team, Some(&root))}),
                )
            }
            _ => return Ok(None),
        }))
    }
}
