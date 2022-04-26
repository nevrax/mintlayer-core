// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://spdx.org/licenses/MIT
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Author(s): L. Kuklinek

use core::time::Duration;
use core::future::Future;
use tokio::sync::{broadcast, mpsc};
use tokio::task;

use logging::log;

use crate::subsystem::{Subsystem, CallRequest, ShutdownRequest, Handle, SubsystemConfig};

/// Manager configuration options.
pub struct ManagerConfig {
    /// Subsystem manager name
    name: &'static str,
    /// Shutdown timeout. Set to `None` for no (i.e. unlimited) timeout.
    shutdown_timeout: Option<Duration>,
}

impl ManagerConfig {
    /// Default shutdown timeout.
    const DEFAULT_SHUTDOWN_TIMEOUT: Option<Duration> = if cfg!(all(feature = "time", not(loom))) {
        Some(Duration::from_secs(20))
    } else {
        None
    };

    fn named(name: &'static str) -> Self {
        Self {
            name,
            shutdown_timeout: Self::DEFAULT_SHUTDOWN_TIMEOUT,
        }
    }
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            name: "<manager>",
            shutdown_timeout: Self::DEFAULT_SHUTDOWN_TIMEOUT,
        }
    }
}

/// Top-level subsystem manager.
///
/// An application is composed of a number of long-lived subsystems. The [Manager] type starts
/// and manages the life cycle of the subsystems. Whenever a subsystem exits, all other subsystems
/// are requested to terminate and the manager is shut down.
pub struct Manager {
    // Manager name
    name: &'static str,

    // Used by the manager to order all subsystems to shut down.
    shutdown_request_tx: broadcast::Sender<()>,

    // Used by a subsystem to notify the manager it is shutting down. This is taken as a command
    // for all subsystems to shut down. Shutdown completion is detected by all senders having closed
    // this channel.
    shutting_down_tx: mpsc::Sender<()>,
    shutting_down_rx: mpsc::Receiver<()>,

    // Shutdown timeout settings
    shutdown_timeout: Option<Duration>,
}

impl Manager {
    /// Initialise a new subsystem manager.
    pub fn new(name: &'static str) -> Self {
        Self::new_with_config(ManagerConfig::named(name))
    }

    /// Initialise a new subsystem manager.
    pub fn new_with_config(config: ManagerConfig) -> Self {
        let ManagerConfig {
            name,
            shutdown_timeout,
        } = config;
        log::info!("Initialising subsystem manager {}", name);

        let (shutdown_request_tx, _shutdown_request_rx) = broadcast::channel(1);
        let (shutting_down_tx, shutting_down_rx) = mpsc::channel(1);

        Self {
            name,
            shutdown_request_tx,
            shutting_down_tx,
            shutting_down_rx,
            shutdown_timeout,
        }
    }

    /// Start a raw subsystem.
    ///
    /// Gives full control over how shutdown and call requests are handled. If this is not
    /// required, use [Manager::start] instead. A subsystem has to handle shutdown and call
    /// requests. It can also react to external IO events. If the subsystem handles *only* calls
    /// and shutdown requests without interaction with any additional IO and does not need custom
    /// shutdown logic, use [Manager::start].
    ///
    /// A typical skeleton of a subsystem looks like this:
    /// ```no_run
    /// # let manager = subsystem::Manager::new("app");
    /// let subsystem = manager.start_raw("my-subsystem", |mut call, mut shutdown| async move {
    ///     loop {
    ///         tokio::select! {
    ///             // Shutdown received, break out of the loop.
    ///             () = shutdown.recv() => { break; }
    ///             // Handle calls. An object representing the subsystem is passed in.
    ///             func = call.recv() => { func(todo!("put an argument here")); }
    ///             // Handle any other IO events here
    ///         };
    ///     }
    /// });
    /// # let _ = subsystem.call(|()| ());  // Fix the call type to avoid amnbiguity.
    /// ```
    pub fn start_raw_with_config<T: 'static + Send, F: 'static + Send + Future<Output = ()>>(
        &self,
        config: SubsystemConfig,
        subsystem: impl 'static + Send + FnOnce(CallRequest<T>, ShutdownRequest) -> F,
    ) -> Handle<T> {
        // Name strings
        let manager_name = self.name;
        let subsys_name = config.subsystem_name;
        // Shutdown-related channels
        let shutting_down_tx = self.shutting_down_tx.clone();
        let shutdown_rq = ShutdownRequest(self.shutdown_request_tx.subscribe());
        // Call related channels
        let (action_tx, action_rx) = mpsc::channel(config.call_queue_capacity);
        let call_rq = CallRequest(action_rx);

        task::spawn(async move {
            log::info!("Subsystem {}/{} started", manager_name, subsys_name);

            // Perform the subsystem task.
            subsystem(call_rq, shutdown_rq).await;

            // Signal the intent to shut down to the other parts of the application.
            shutting_down_tx.send(()).await.expect("Subsystem outlived the manager!?");

            log::info!("Subsystem {}/{} terminated", manager_name, subsys_name);

            // Close the channel to signal the completion of the shutdown.
            std::mem::drop(shutting_down_tx);
        });

        Handle::new(action_tx)
    }

    /// Start a passive subsystem.
    ///
    /// A passive subsystem does not interact with the environment on its own. It only serves calls
    /// from other subsystems. A hook to be invoked on shutdown can be specified by means of the
    /// [Subsystem] trait.
    pub fn start_with_config<S: 'static + Send + Subsystem>(
        &self,
        config: SubsystemConfig,
        mut subsys: S,
    ) -> Handle<S> {
        self.start_raw_with_config(config, |mut call_rq, mut shutdown_rq| async move {
            loop {
                tokio::select! {
                    () = shutdown_rq.recv() => { break; }
                    call = call_rq.recv() => { call(&mut subsys).await; }
                }
            }
            subsys.shutdown().await;
        })
    }

    /// Start a raw subsystem. See [Manager::start_raw_with_config].
    pub fn start_raw<T: 'static + Send, F: 'static + Send + Future<Output = ()>>(
        &self,
        name: &'static str,
        subsystem: impl 'static + Send + FnOnce(CallRequest<T>, ShutdownRequest) -> F,
    ) -> Handle<T> {
        self.start_raw_with_config(SubsystemConfig::named(name), subsystem)
    }

    /// Start a passive subsystem. See [Manager::start_with_config].
    pub fn start<S: 'static + Send + Subsystem>(&self, name: &'static str, subsys: S) -> Handle<S> {
        self.start_with_config(SubsystemConfig::named(name), subsys)
    }

    /// Install termination signal handlers.
    ///
    /// This adds a subsystem that listens for the Ctrl-C signal and exits once it is received,
    /// signalling all other subsystems and the whole manager to shut down.
    #[cfg(not(loom))]
    pub fn install_signal_handlers(&self) {
        self.start_raw(
            "ctrl-c",
            |mut call_rq: CallRequest<()>, mut shutdown_rq| async move {
                tokio::select! {
                    ctrl_c_signal = tokio::signal::ctrl_c() => {
                        if ctrl_c_signal.is_err() {
                            log::info!("Ctrl-C signal handler failed");
                        }
                    }
                    () = shutdown_rq.recv() => {},
                    call = call_rq.recv() => { call(&mut ()); }
                };
            },
        );
    }

    /// Issue an asynchronous shutdown request
    pub async fn initiate_shutdown(&self) {
        self.shutting_down_tx.send(()).await.expect("Shutdown receiver not existing")
    }

    async fn wait_for_shutdown(mut shutting_down_rx: mpsc::Receiver<()>) {
        // Wait for the subsystems to go down, signalled by closing the shutting_down channel.
        while let Some(()) = shutting_down_rx.recv().await {}
    }

    #[allow(unused)]
    async fn wait_for_shutdown_with_timeout(
        name: &'static str,
        shutting_down_rx: mpsc::Receiver<()>,
        timeout: Option<Duration>,
    ) {
        let shutdown_future = Self::wait_for_shutdown(shutting_down_rx);
        if let Some(timeout) = timeout {
            cfg_if::cfg_if! {
                if #[cfg(all(feature = "time", not(loom)))] {
                    // Wait for shutdown under a timeout
                    if let Err(elapsed) = tokio::time::timeout(timeout, shutdown_future).await {
                        log::error!("Manager {} shutdown timed out", name);
                    }
                } else {
                    // Timeout was requested but is not supported
                    if cfg!(not(feature = "time")) {
                        log::error!("Shutdown timeout support not compiled in");
                    } else if cfg!(loom) {
                        log::warn!("Shutdown timeout disabled under loom");
                    }
                    shutdown_future.await
                }
            }
        } else {
            // No timeout requested, just wait for shutdown
            shutdown_future.await
        }
    }

    /// Run the application main task.
    ///
    /// Completes when all the subsystems are fully shut down.
    pub async fn main(mut self) {
        // The main task just waits for the shutdown signal and coordinates the cleanup at the end.
        // All other functionality is performed by subsystems.

        log::info!("Manager {} running", self.name);

        // Signal the manager is shut down so it does not wait for itself
        std::mem::drop(self.shutting_down_tx);

        // Wait for a subsystem to shut down.
        self.shutting_down_rx
            .recv()
            .await
            .unwrap_or_else(|| log::info!("Manager {}: all subsystems already down", self.name));

        log::info!("Manager {} shutting down", self.name);

        // Order all the remaining subsystems to shut down.
        let _ = self.shutdown_request_tx.send(());

        // Wait for the subsystems to go down.
        Self::wait_for_shutdown_with_timeout(
            self.name,
            self.shutting_down_rx,
            self.shutdown_timeout,
        )
        .await;

        log::info!("Manager {} terminated", self.name);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[cfg(all(feature = "time", not(loom)))]
    #[tokio::test]
    async fn shutdown_timeout() {
        testing_logger::setup();

        let man = Manager::new_with_config(ManagerConfig {
            name: "timeout_test",
            shutdown_timeout: Some(Duration::from_secs(1)),
        });

        man.start_raw(
            "does_not_want_to_exit",
            |_call_rq: CallRequest<()>, _shut_rq| std::future::pending(),
        );
        man.initiate_shutdown().await;
        man.main().await;

        testing_logger::validate(|logs| {
            assert!(logs.iter().any(|entry| entry.body.contains("shutdown timed out")));
        });
    }
}