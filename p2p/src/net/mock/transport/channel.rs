// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use tokio::sync::mpsc;

use crate::net::mock::{types::Message, SocketService, TransportService};

enum SocketCommand {
    Connect(u64),
    Bind(u64),
}

// spawn mock network ("super-backend") into the background
//
// Sending `SocketCommand` events via the TX channel is akin to excuting a system call for TCP-based approach
lazy_static::lazy_static! {
    static ref NETWORK_HANDLE: mpsc::UnboundedSender<SocketCommand> = {
        let (tx, mut rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Some(cmd) => match cmd {
                        SocketCommand::Connect(address) => {
                            // TODO: check if peer with `address` has connected
                        }
                        SocketCommand::Bind(address) => {
                            // TODO: check if `address` is free and if it is, bind this peer to that address
                        }
                    }
                    None => { panic!("") },
                }
            }
        });

        tx
    };
}

pub struct ChannelService<T: TransportService> {
    _marker: std::marker::PhantomData<fn() -> T>,
}

pub struct ChannelSocket<T: TransportService> {
    _marker: std::marker::PhantomData<fn() -> T>,
}

#[async_trait::async_trait]
impl<T: TransportService> TransportService for ChannelService<T> {
    type Socket = ChannelSocket<T>;
    type Address = u64;

    /// Create a new socket and bind it to `address`
    async fn bind(address: Self::Address) -> crate::Result<Self::Socket> {
        let tx: mpsc::UnboundedSender<SocketCommand> = NETWORK_HANDLE.clone();

		todo!();
    }

    /// Create new socket and try to establish a connection to `address`
    async fn connect(address: Self::Address) -> crate::Result<Self::Socket> {
        todo!();
    }
}

#[async_trait::async_trait]
impl<T: TransportService + 'static> SocketService<T> for ChannelSocket<T> {

    async fn accept(&mut self) -> crate::Result<(T::Socket, T::Address)> {
        todo!();
    }

    async fn connect(&mut self) -> crate::Result<T::Socket> {
        todo!();
    }

    async fn send(&mut self, msg: Message) -> Result<(), std::io::Error> {
        todo!();
    }

    async fn recv(&mut self) -> Result<Option<Message>, std::io::Error> {
        todo!();
    }

    fn local_addr(&self) -> crate::Result<T::Address> {
        todo!();
    }
}
