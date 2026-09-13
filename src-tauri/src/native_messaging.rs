use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

use crate::{
    database::{Database, TaskInput},
    error::{AppError, AppResult},
};

const MAX_MESSAGE_BYTES: usize = 1_048_576;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum Request {
    Ping,
    Capture {
        title: String,
        #[serde(default)]
        notes: String,
        #[serde(default = "default_source")]
        source_type: String,
        source_url: Option<String>,
        source_label: Option<String>,
        source_author: Option<String>,
        source_excerpt: Option<String>,
        screenshot_data_url: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    ok: bool,
    task_id: Option<String>,
    error: Option<String>,
}

fn default_source() -> String {
    "web".into()
}

pub fn run() -> AppResult<()> {
    let database = Database::open_default()?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    loop {
        let Some(payload) = read_message(&mut input)? else {
            break;
        };
        let response = match serde_json::from_slice::<Request>(&payload) {
            Ok(Request::Ping) => Response {
                ok: true,
                task_id: None,
                error: None,
            },
            Ok(Request::Capture {
                title,
                notes,
                source_type,
                source_url,
                source_label,
                source_author,
                source_excerpt,
                screenshot_data_url,
            }) => {
                match database.create_task(TaskInput {
                    title,
                    notes,
                    status: "inbox".into(),
                    priority: "mid".into(),
                    project_id: None,
                    source_type,
                    source_url,
                    source_label,
                    source_author,
                    source_excerpt,
                    due_at: None,
                    remind_at: None,
                    screenshot_data_url,
                    screenshots: Vec::new(),
                    audio_attachments: Vec::new(),
                    links: Vec::new(),
                    file_attachments: Vec::new(),
                }) {
                    Ok(task) => Response {
                        ok: true,
                        task_id: Some(task.id),
                        error: None,
                    },
                    Err(error) => Response {
                        ok: false,
                        task_id: None,
                        error: Some(error.to_string()),
                    },
                }
            }
            Err(error) => Response {
                ok: false,
                task_id: None,
                error: Some(format!("Invalid native message: {error}")),
            },
        };
        write_message(&mut output, &serde_json::to_vec(&response)?)?;
    }
    Ok(())
}

fn read_message(reader: &mut impl Read) -> AppResult<Option<Vec<u8>>> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let length = u32::from_ne_bytes(length) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(AppError::InvalidInput(
            "Native message exceeds the 1 MiB Firefox limit".into(),
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_message(writer: &mut impl Write, payload: &[u8]) -> AppResult<()> {
    writer.write_all(&(payload.len() as u32).to_ne_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}
