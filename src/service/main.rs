// WebSocket Service
//
// You can install and uninstall this service using other example programs.
// All commands mentioned below shall be executed in Command Prompt with Administrator privileges.
//
// Service installation: `install_service.exe`
// Service uninstallation: `uninstall_service.exe`
//
// Start the service: `net start ws_omega2_service`
// Stop the service: `net stop ws_omega2_service`
//
// WebSocket server sends data to local UDP port 1337 once a second.
// You can verify that service works by running netcat, i.e: `ncat -ul 1337`.

#[cfg(windows)]
fn main() -> windows_service::Result<()> {
    service::run()
}

#[cfg(not(windows))]
fn main() {
    panic!("This program is only intended to run on Windows.");
}

#[cfg(windows)]
mod service {
    use std::{
        ffi::OsString,
        net::SocketAddr,
        time::Duration,
    };
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher, Result,
    };
    use log::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::{accept_async, tungstenite, tungstenite::Error, tungstenite::Message};

    const SERVICE_NAME: &str = "ws_service";
    const ADDRESS: &str = "127.0.0.1:1337";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    pub fn run() -> Result<()> {
        // Register generated `ffi_service_main` with the system and start the service, blocking
        // this thread until the service is stopped.
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }

    // Generate the windows service boilerplate.
    // The boilerplate contains the low-level service entry function (ffi_service_main) that parses
    // incoming service arguments into Vec<OsString> and passes them to user defined service
    // entry (my_service_main).
    define_windows_service!(ffi_service_main, my_service_main);

    // Service entry function which is called on background thread by the system with service
    // parameters. There is no stdout or stderr at this point so make sure to configure the log
    // output to file if needed.
    pub fn my_service_main(_arguments: Vec<OsString>) {
        if let Err(_e) = run_service() {
            panic!("Error in run_service");
        }
    }

    async fn accept_connection(peer: SocketAddr, stream: TcpStream) {
        if let Err(e) = handle_connection(peer, stream).await {
            match e {
                Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
                err => error!("Error processing connection: {:?}", err),
            }
        }
    }

    async fn handle_connection(peer: SocketAddr, stream: TcpStream) -> tungstenite::Result<()> {
        let ws_stream = accept_async(stream).await.expect("Failed to accept");
        info!("New WebSocket connection: {}", peer);
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut interval = tokio::time::interval(Duration::from_millis(1000));

        // Echo incoming WebSocket messages and send a message periodically every second.

        loop {
            tokio::select! {
                msg = ws_receiver.next() => {
                    match msg {
                        Some(msg) => {
                            let msg = msg?;
                            if msg.is_text() ||msg.is_binary() {
                                ws_sender.send(msg).await?;
                            } else if msg.is_close() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = interval.tick() => {
                    ws_sender.send(Message::Text("tick".to_owned())).await?;
                }
            }
        }

        Ok(())
    }

    #[tokio::main]
    async fn main() {
        let listener = TcpListener::bind(ADDRESS).await.expect("Can't listen");
        info!("Listening on: {}", ADDRESS);

        while let Ok((stream, _)) = listener.accept().await {
            let peer = stream.peer_addr().expect("connected streams should have a peer address");
            info!("Peer address: {}", peer);

            tokio::spawn(accept_connection(peer, stream));
        }
    }

    pub fn run_service() -> Result<()> {
        // Define system service event handler that will be receiving service events.
        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                // Notifies a service to report its current status information to the service
                // control manager. Always return NoError even if not implemented.
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,

                // Handle stop
                ServiceControl::Stop => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };
        // Register system service event handler.
        // The returned status handle should be used to report service status changes to the system.
        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        // Tell the system that service is running
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        main();

        // Tell the system that service has stopped.
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }
}
