//! Shun-owned Linux resident operations. The single-instance plugin still owns the process.

use crate::cli::Command;
use parking_lot::Mutex;
use std::sync::mpsc;
use tauri::Manager;
use tokio::sync::oneshot;
use zbus::blocking::{connection::Builder, Connection};

pub(crate) const BUS_NAME: &str = "com.yukimemi.shun.Control";
pub(crate) const OBJECT_PATH: &str = "/com/yukimemi/shun/Control";
pub(crate) const INTERFACE: &str = "com.yukimemi.shun.Control1";

type CommandResult = Result<(), String>;
struct Request {
    command: Command,
    reply: Option<oneshot::Sender<CommandResult>>,
}

/// One FIFO for D-Bus, startup arguments, and the single-instance ownership callback.
/// It is installed before plugins initialize, so an early secondary invocation can queue
/// work without blocking the GUI thread while setup is still in progress.
pub(crate) struct CommandQueue {
    sender: mpsc::Sender<Request>,
    receiver: Mutex<Option<mpsc::Receiver<Request>>>,
}

impl Default for CommandQueue {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver: Mutex::new(Some(receiver)),
        }
    }
}

impl CommandQueue {
    pub(crate) fn submit(&self, command: Command) -> Result<(), String> {
        self.sender
            .send(Request {
                command,
                reply: None,
            })
            .map_err(|_| "resident command worker is unavailable".to_string())
    }

    #[cfg(test)]
    fn submit_and_wait(
        &self,
        command: Command,
    ) -> Result<oneshot::Receiver<CommandResult>, String> {
        let (reply, result) = oneshot::channel();
        self.sender
            .send(Request {
                command,
                reply: Some(reply),
            })
            .map_err(|_| "resident command worker is unavailable".to_string())?;
        Ok(result)
    }

    fn start(
        &self,
        execute: impl Fn(Command) -> CommandResult + Send + 'static,
    ) -> Result<(), String> {
        let receiver = self
            .receiver
            .lock()
            .take()
            .ok_or_else(|| "resident command worker is already started".to_string())?;
        std::thread::Builder::new()
            .name("shun-control".to_string())
            .spawn(move || {
                for request in receiver {
                    let result = execute(request.command);
                    if let Err(error) = &result {
                        log::warn!("resident command: {error}");
                    }
                    // A disconnected caller never cancels or replays an accepted operation.
                    if let Some(reply) = request.reply {
                        let _ = reply.send(result);
                    }
                }
            })
            .map_err(|e| format!("start resident command worker: {e}"))?;
        Ok(())
    }
}

struct Control {
    sender: mpsc::Sender<Request>,
}

impl Control {
    async fn execute(&self, command: Command) -> zbus::fdo::Result<()> {
        let (reply, result) = oneshot::channel();
        self.sender
            .send(Request {
                command,
                reply: Some(reply),
            })
            .map_err(|_| {
                zbus::fdo::Error::Failed("resident command worker is unavailable".into())
            })?;
        result
            .await
            .map_err(|_| {
                zbus::fdo::Error::Failed(
                    "resident command worker stopped; delivery is uncertain".into(),
                )
            })?
            .map_err(zbus::fdo::Error::Failed)
    }
}

// Preserve bus receive order as well as the FIFO execution order. In particular, do
// not spawn one independently scheduled operation per method, and never retry one.
#[zbus::interface(name = "com.yukimemi.shun.Control1", spawn = false)]
impl Control {
    async fn show(&self) -> zbus::fdo::Result<()> {
        self.execute(Command::Show).await
    }

    async fn toggle(&self) -> zbus::fdo::Result<()> {
        self.execute(Command::Toggle).await
    }

    async fn hotkey(&self, key: &str) -> zbus::fdo::Result<()> {
        self.execute(Command::Hotkey(key.to_string())).await
    }
}

// Retaining the connection in Tauri state retains both the bus name and object server.
struct ResidentConnection {
    _connection: Connection,
}

pub(crate) fn start(app: &tauri::AppHandle) -> Result<(), String> {
    let queue = app.state::<CommandQueue>();
    let handle = app.clone();
    queue.start(move |command| crate::execute_command(&handle, command))?;
    let connection = Builder::session()
        .and_then(|builder| {
            builder.serve_at(
                OBJECT_PATH,
                Control {
                    sender: queue.sender.clone(),
                },
            )
        })
        .and_then(|builder| builder.name(BUS_NAME))
        .and_then(|builder| builder.build())
        .map_err(|e| format!("start resident control: {e}"))?;
    if !app.manage(ResidentConnection {
        _connection: connection,
    }) {
        return Err("resident control is already started".into());
    }
    Ok(())
}

/// Probe without activation, then address the unique owner so a disappearing resident
/// cannot accidentally activate a replacement. Only definite absence permits GUI startup.
pub(crate) fn try_forward(command: &Command) -> Result<bool, String> {
    if matches!(command, Command::InstallDesktop) {
        return Err("install-desktop is a local command".into());
    }
    let connection = Connection::session().map_err(|e| format!("connect resident control: {e}"))?;
    forward_on(&connection, command)
}

fn forward_on(connection: &Connection, command: &Command) -> Result<bool, String> {
    let bus = zbus::blocking::fdo::DBusProxy::new(connection)
        .map_err(|e| format!("query resident control: {e}"))?;
    let owner = match bus.get_name_owner(BUS_NAME.try_into().expect("valid control bus name")) {
        Ok(owner) => owner,
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => return Ok(false),
        Err(e) => return Err(format!("query resident control owner: {e}")),
    };
    let result = match command {
        Command::Show => connection.call_method(
            Some(owner.as_str()),
            OBJECT_PATH,
            Some(INTERFACE),
            "Show",
            &(),
        ),
        Command::Toggle => connection.call_method(
            Some(owner.as_str()),
            OBJECT_PATH,
            Some(INTERFACE),
            "Toggle",
            &(),
        ),
        Command::Hotkey(key) => connection.call_method(
            Some(owner.as_str()),
            OBJECT_PATH,
            Some(INTERFACE),
            "Hotkey",
            &(key.as_str(),),
        ),
        Command::InstallDesktop => return Err("install-desktop is a local command".into()),
        Command::Resident => return Ok(true),
    };
    match result {
        Ok(reply) => {
            reply
                .body()
                .deserialize::<()>()
                .map_err(|e| format!("invalid resident control reply (not replayed): {e}"))?;
            Ok(true)
        }
        // Once delivery is attempted, every error is terminal: even an application
        // returning a standard absence error must not trigger a replay.
        Err(e) => Err(format!(
            "resident control command failed (not replayed): {e}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command as ProcessCommand, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A private bus with no production activation directories or desktop dependency.
    struct TestBus {
        child: Child,
        address: String,
        _directory: tempfile::TempDir,
    }

    impl TestBus {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let services = directory.path().join("services");
            std::fs::create_dir(&services).unwrap();
            std::fs::write(
                services.join(format!("{BUS_NAME}.service")),
                format!("[D-BUS Service]\nName={BUS_NAME}\nExec=/bin/false\n"),
            )
            .unwrap();
            let config = directory.path().join("bus.conf");
            std::fs::write(
                &config,
                format!(
                    "<busconfig><type>session</type><listen>unix:tmpdir={}</listen>\
                 <servicedir>{}</servicedir><policy context=\"default\">\
                 <allow send_destination=\"*\"/><allow receive_sender=\"*\"/>\
                 <allow own=\"*\"/></policy></busconfig>",
                    directory.path().display(),
                    services.display()
                ),
            )
            .unwrap();
            let mut child = ProcessCommand::new("dbus-daemon")
                .arg("--nofork")
                .arg("--print-address=1")
                .arg("--config-file")
                .arg(config)
                .stdout(Stdio::piped())
                .spawn()
                .expect("tests require dbus-daemon");
            let mut address = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut address)
                .unwrap();
            assert!(!address.trim().is_empty(), "private bus did not start");
            Self {
                child,
                address: address.trim().to_string(),
                _directory: directory,
            }
        }

        fn connect(&self) -> Connection {
            Builder::address(self.address.as_str())
                .unwrap()
                .build()
                .unwrap()
        }

        fn serve(&self, queue: &CommandQueue) -> Connection {
            Builder::address(self.address.as_str())
                .unwrap()
                .serve_at(
                    OBJECT_PATH,
                    Control {
                        sender: queue.sender.clone(),
                    },
                )
                .unwrap()
                .name(BUS_NAME)
                .unwrap()
                .build()
                .unwrap()
        }
    }

    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn absent_owner_does_not_activate_an_available_service() {
        let bus = TestBus::new();
        // An activation attempt would execute /bin/false and return an error, not false.
        assert!(!forward_on(&bus.connect(), &Command::Show).unwrap());
        assert!(!forward_on(&bus.connect(), &Command::Resident).unwrap());
    }

    #[test]
    fn bootstrap_with_an_owner_exits_without_executing_any_operation() {
        let bus = TestBus::new();
        let queue = CommandQueue::default();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&count);
        queue
            .start(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Err("bootstrap must not queue an operation".into())
            })
            .unwrap();
        let _resident = bus.serve(&queue);
        assert!(forward_on(&bus.connect(), &Command::Resident).unwrap());
        assert_eq!(count.load(Ordering::SeqCst), 0);
        // These are the actual routing functions used by setup and the ownership
        // callback, including when the ownership name predates the control name.
        assert_eq!(
            crate::cli::startup_command(&[crate::cli::RESIDENT_ARG]),
            None
        );
        assert_eq!(
            crate::cli::secondary_command(&[crate::cli::RESIDENT_ARG]),
            None
        );
    }

    #[test]
    fn public_hotkey_executes_once_and_operation_errors_never_allow_fallback() {
        let bus = TestBus::new();
        let queue = CommandQueue::default();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&count);
        let mut config = Config::default();
        config.keybindings.launch = "F10".into();
        queue
            .start(move |command| match command {
                Command::Hotkey(key) => crate::execute_hotkey(&config, &key, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
                _ => Err("main window is unavailable".into()),
            })
            .unwrap();
        let _resident = bus.serve(&queue);
        let client = bus.connect();
        assert!(forward_on(&client, &Command::Hotkey("F10".into())).unwrap());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        for command in [
            Command::Hotkey("invalid key".into()),
            Command::Hotkey("F11".into()),
            Command::Show,
            Command::Toggle,
            Command::InstallDesktop,
        ] {
            assert!(forward_on(&client, &command).is_err(), "{command:?}");
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn queued_operations_are_fifo_and_failure_does_not_drop_the_next_request() {
        let queue = CommandQueue::default();
        let (entered, first_entered) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let mut config = Config::default();
        config.keybindings.launch = "F10".into();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&count);
        queue
            .start(move |command| {
                let Command::Hotkey(key) = command else {
                    panic!("expected hotkey")
                };
                crate::execute_hotkey(&config, &key, || {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        entered.send(()).unwrap();
                        released.recv().unwrap();
                    }
                    Ok(())
                })
            })
            .unwrap();
        let first = queue
            .submit_and_wait(Command::Hotkey("F10".into()))
            .unwrap();
        first_entered.recv().unwrap();
        let invalid = queue
            .submit_and_wait(Command::Hotkey("F11".into()))
            .unwrap();
        let mut last = queue
            .submit_and_wait(Command::Hotkey("F10".into()))
            .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(matches!(
            last.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        release.send(()).unwrap();
        first.blocking_recv().unwrap().unwrap();
        assert!(invalid.blocking_recv().unwrap().is_err());
        last.blocking_recv().unwrap().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn disconnect_after_acceptance_is_terminal_and_does_not_replay() {
        let bus = TestBus::new();
        let queue = CommandQueue::default();
        let (entered, accepted) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&calls);
        let mut config = Config::default();
        config.keybindings.launch = "F10".into();
        queue
            .start(move |command| {
                let Command::Hotkey(key) = command else {
                    panic!("expected hotkey")
                };
                crate::execute_hotkey(&config, &key, || {
                    count.fetch_add(1, Ordering::SeqCst);
                    entered.send(()).unwrap();
                    released.recv().unwrap();
                    Ok(())
                })
            })
            .unwrap();
        let resident = bus.serve(&queue);
        let client = bus.connect();
        let forwarding =
            std::thread::spawn(move || forward_on(&client, &Command::Hotkey("F10".into())));
        accepted.recv().unwrap();
        resident.close().unwrap();
        release.send(()).unwrap();
        assert!(forwarding.join().unwrap().is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unavailable_worker_is_an_observable_public_error() {
        let bus = TestBus::new();
        let queue = CommandQueue::default();
        drop(queue.receiver.lock().take());
        let _resident = bus.serve(&queue);
        assert!(forward_on(&bus.connect(), &Command::Show).is_err());
        assert!(queue.submit(Command::Show).is_err());
    }
}
