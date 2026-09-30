use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppArgs {
    pub smoke: bool,
    pub hub_connection: Option<RunnableEntrypointHubConnection>,
    pub connection_error: Option<String>,
    pub hub_data_dir: Option<PathBuf>,
}

/// What the command line asks the binary to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedCommand {
    Run(AppArgs),
    Help,
    Version,
}

pub const fn usage() -> &'static str {
    "usage: botster-tui [--smoke]\n\n\
     The Hub host supplies the connection in BOTSTER_HUB_CONNECTION.\n\n\
     Options:\n\
     \x20 --smoke        Print a startup smoke message and exit\n\
     \x20 -h, --help     Show this help\n\
     \x20 -V, --version  Show the version"
}

impl AppArgs {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<ParsedCommand, String> {
        let hub_data_dir = std::env::var_os("BOTSTER_HUB_DATA_DIR");
        Self::parse_with_environment(
            args,
            std::env::var_os("BOTSTER_HUB_CONNECTION"),
            hub_data_dir,
        )
    }

    pub(super) fn parse_with_environment(
        args: impl IntoIterator<Item = String>,
        hub_connection: Option<std::ffi::OsString>,
        hub_data_dir: Option<std::ffi::OsString>,
    ) -> Result<ParsedCommand, String> {
        let mut parsed = Self::default();
        for arg in args {
            match arg.as_str() {
                "--smoke" => parsed.smoke = true,
                "-h" | "--help" => return Ok(ParsedCommand::Help),
                "-V" | "--version" => return Ok(ParsedCommand::Version),
                _ => return Err(format!("unknown option: {arg}")),
            }
        }
        let (connection, connection_error) = parse_hub_connection(hub_connection);
        parsed.hub_connection = connection;
        parsed.connection_error = connection_error;
        parsed.hub_data_dir = hub_data_dir.map(PathBuf::from);
        Ok(ParsedCommand::Run(parsed))
    }

    pub(super) fn daemon_endpoint(&self) -> Option<DaemonEndpoint> {
        self.hub_connection
            .as_ref()
            .map(|connection| match &connection.transport {
                RunnableEntrypointHubConnectionTransport::UnixSocket { path } => {
                    DaemonEndpoint::new(path)
                }
            })
    }
}

pub(super) fn parse_hub_connection(
    value: Option<std::ffi::OsString>,
) -> (Option<RunnableEntrypointHubConnection>, Option<String>) {
    let Some(value) = value else {
        return (None, Some("BOTSTER_HUB_CONNECTION is required".to_string()));
    };
    let value = match value.into_string() {
        Ok(value) => value,
        Err(_) => {
            return (
                None,
                Some("BOTSTER_HUB_CONNECTION must contain UTF-8 JSON".to_string()),
            );
        }
    };
    let connection = match serde_json::from_str::<RunnableEntrypointHubConnection>(&value) {
        Ok(connection) => connection,
        Err(error) => {
            return (
                None,
                Some(format!("BOTSTER_HUB_CONNECTION is malformed: {error}")),
            );
        }
    };
    if let Err(error) = connection.validate() {
        return (
            None,
            Some(format!("BOTSTER_HUB_CONNECTION is invalid: {error}")),
        );
    }
    (Some(connection), None)
}

#[cfg(test)]
pub(super) fn parse_shared_session_id(value: Option<std::ffi::OsString>) -> Result<String, String> {
    let Some(value) = value else {
        return Err("BOTSTER_SHARED_SESSION_ID is required".to_string());
    };
    let value = value
        .into_string()
        .map_err(|_| "BOTSTER_SHARED_SESSION_ID must contain UTF-8".to_string())?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("BOTSTER_SHARED_SESSION_ID is required".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn smoke_message() -> &'static str {
    SMOKE_MESSAGE
}
