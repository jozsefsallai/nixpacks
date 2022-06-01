use std::collections::HashMap;

use super::Provider;
use crate::nixpacks::{
    app::App,
    environment::{Environment, EnvironmentVariables},
    nix::Pkg,
    phase::{BuildPhase, InstallPhase, SetupPhase, StartPhase},
};
use anyhow::Result;
use regex::Regex;

const AVAILABLE_ELIXIR_VERSIONS: &[(f64, &str)] = &[
    (1.9, "elixir_1_9"),
    (1.10, "elixir_1_10"),
    (1.11, "elixir_1_11"),
    (1.12, "elixir_1_12"),
    (1.13, "elixir"),
];
const DEFAULT_ELIXIR_PKG_NAME: &'static &str = &"elixir";

#[derive(Debug)]
pub struct MixProject {
    pub app_name: Option<String>,
    pub elixir_version: Option<String>,
    pub is_escript: bool,
}

pub struct ElixirProvider {}

impl Provider for ElixirProvider {
    fn name(&self) -> &str {
        "elixir"
    }

    fn detect(&self, app: &App, _env: &Environment) -> Result<bool> {
        Ok(app.includes_file("mix.exs"))
    }

    fn setup(&self, app: &App, _env: &Environment) -> Result<Option<SetupPhase>> {
        let mix_project = ElixirProvider::parse_mix_project(app)?;
        let nix_pkg = ElixirProvider::get_nix_elixir_pkg(mix_project)?;

        Ok(Some(SetupPhase::new(vec![Pkg::new(&nix_pkg)])))
    }

    fn install(&self, _app: &App, _env: &Environment) -> Result<Option<InstallPhase>> {
        Ok(Some(InstallPhase::new("mix deps.get".to_string())))
    }

    fn build(&self, app: &App, _env: &Environment) -> Result<Option<BuildPhase>> {
        let mix_project = ElixirProvider::parse_mix_project(app)?;

        if let Some(project) = mix_project {
            if project.is_escript && project.app_name.is_some() {
                return Ok(Some(BuildPhase::new("mix escript.build".to_string())));
            }
        }

        Ok(None)
    }

    fn start(&self, app: &App, _env: &Environment) -> Result<Option<StartPhase>> {
        let mix_project = ElixirProvider::parse_mix_project(app)?;

        if let Some(project) = mix_project {
            if project.is_escript && project.app_name.is_some() {
                return Ok(Some(StartPhase::new(format!(
                    "./{}",
                    project.app_name.unwrap()[1..].to_string()
                ))));
            }
        }

        Ok(Some(StartPhase::new("mix run --no-halt".to_string())))
    }

    fn environment_variables(
        &self,
        _app: &App,
        _env: &Environment,
    ) -> Result<Option<EnvironmentVariables>> {
        Ok(Some(ElixirProvider::get_elixir_environment_variables()))
    }
}

impl ElixirProvider {
    pub fn get_elixir_environment_variables() -> EnvironmentVariables {
        EnvironmentVariables::from([("MIX_ENV".to_string(), "prod".to_string())])
    }

    pub fn parse_mix_project(app: &App) -> Result<Option<MixProject>> {
        if !app.includes_file("mix.exs") {
            return Ok(None);
        }

        let mix_exs_contents = app.read_file("mix.exs")?;

        let re_superglobal = Regex::new("@(.+)\\s\"(.+)\"").unwrap();
        let re_mix_property = Regex::new("(?m)([^\\s]+):\\s([^,]+)(?:,|$)").unwrap();

        let mut superglobals: HashMap<String, String> = HashMap::new();

        for capture in re_superglobal.captures_iter(mix_exs_contents.as_str()) {
            if capture.len() != 3 {
                continue;
            }

            let key = capture[1].to_string();
            let value = capture[2].to_string();

            superglobals.insert(key, value);
        }

        let mut app_name = None;
        let mut elixir_version = None;
        let mut is_escript = false;

        for capture in re_mix_property.captures_iter(mix_exs_contents.as_str()) {
            if capture.len() != 3 {
                continue;
            }

            let key = capture[1].to_string();
            let value = capture[2].to_string();

            let parsed_value = ElixirProvider::parse_with_superglobal(&superglobals, &value);

            match key.as_str() {
                "app" => {
                    app_name = parsed_value;
                }
                "elixir" => {
                    elixir_version = parsed_value;
                }
                "escript" => {
                    is_escript = true;
                }
                _ => {}
            }
        }

        let mix_project = MixProject {
            app_name,
            elixir_version,
            is_escript,
        };

        Ok(Some(mix_project))
    }

    fn parse_with_superglobal(
        superglobals: &HashMap<String, String>,
        value: &String,
    ) -> Option<String> {
        if let Some(superglobal) = superglobals.get(value) {
            return Some(superglobal.to_string());
        }

        Some(value.to_string())
    }

    fn get_closest_version(str: &String) -> Result<Option<String>> {
        let re = Regex::new("\\d.\\d").unwrap();

        let version_capture = re.captures(str.as_str());
        if let Some(raw_version) = version_capture {
            let version = raw_version.get(0).unwrap().as_str().parse::<f64>()?;

            let closest_version = AVAILABLE_ELIXIR_VERSIONS
                .iter()
                .find(|(version_f64, _)| version_f64 >= &version);

            if let Some((_, closest_version_str)) = closest_version {
                return Ok(Some(closest_version_str.to_string()));
            }
        }

        Ok(None)
    }

    fn get_nix_elixir_pkg(mix_project: Option<MixProject>) -> Result<String> {
        if let Some(mix_project) = mix_project {
            if let Some(elixir_version) = mix_project.elixir_version {
                if let Some(nix_pkg) = ElixirProvider::get_closest_version(&elixir_version)? {
                    return Ok(nix_pkg);
                }
            }
        }

        Ok(DEFAULT_ELIXIR_PKG_NAME.to_string())
    }
}
