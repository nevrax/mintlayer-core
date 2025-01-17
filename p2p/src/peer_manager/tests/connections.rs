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

use std::{net::SocketAddr, sync::Arc, time::Duration};

use libp2p::{Multiaddr, PeerId};
use tokio::{sync::oneshot, time::timeout};

use common::{chain::config, primitives::semver::SemVer};
use p2p_test_utils::{make_libp2p_addr, make_mock_addr};

use crate::{
    error::{DialError, P2pError, ProtocolError},
    event::SwarmEvent,
    net::{
        self,
        libp2p::Libp2pService,
        mock::{types::MockPeerId, MockService},
        types::{Protocol, ProtocolType},
        ConnectivityService, NetworkingService,
    },
    peer_manager::{
        self,
        tests::{connect_services, default_protocols, make_peer_manager},
    },
};

// try to connect to an address that no one listening on and verify it fails
async fn test_swarm_connect<T: NetworkingService>(bind_addr: T::Address, remote_addr: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let mut swarm = make_peer_manager::<T>(bind_addr, config).await;

    swarm.connect(remote_addr).await.unwrap();

    assert!(std::matches!(
        swarm.peer_connectivity_handle.poll_next().await,
        Ok(net::types::ConnectivityEvent::ConnectionError {
            address: _,
            error: P2pError::DialError(DialError::ConnectionRefusedOrTimedOut)
        })
    ));
}

#[tokio::test]
async fn test_swarm_connect_mock() {
    let bind_addr = make_mock_addr();
    let remote_addr: SocketAddr = "[::1]:1".parse().unwrap();

    test_swarm_connect::<MockService>(bind_addr, remote_addr).await;
}

#[tokio::test]
async fn test_swarm_connect_libp2p() {
    let bind_addr = make_libp2p_addr();
    let remote_addr: Multiaddr =
        "/ip6/::1/tcp/6666/p2p/12D3KooWRn14SemPVxwzdQNg8e8Trythiww1FWrNfPbukYBmZEbJ"
            .parse()
            .unwrap();

    test_swarm_connect::<Libp2pService>(bind_addr, remote_addr).await;
}

// verify that the auto-connect functionality works if the number of active connections
// is below the desired threshold and there are idle peers in the peerdb
async fn test_auto_connect<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::clone(&config)).await;
    let mut swarm2 = make_peer_manager::<T>(addr2, config).await;

    let addr = swarm2.peer_connectivity_handle.local_addr().await.unwrap().unwrap();
    let peer_id = *swarm2.peer_connectivity_handle.peer_id();

    tokio::spawn(async move {
        loop {
            assert!(swarm2.peer_connectivity_handle.poll_next().await.is_ok());
        }
    });

    // "discover" the other networking service
    swarm1.peer_discovered(&[net::types::AddrInfo {
        peer_id,
        ip4: vec![],
        ip6: vec![addr],
    }]);
    swarm1.heartbeat().await.unwrap();

    assert_eq!(swarm1.pending.len(), 1);
    assert!(std::matches!(
        swarm1.peer_connectivity_handle.poll_next().await,
        Ok(net::types::ConnectivityEvent::OutboundAccepted { .. })
    ));
}

#[tokio::test]
async fn test_auto_connect_libp2p() {
    test_auto_connect::<Libp2pService>(make_libp2p_addr(), make_libp2p_addr()).await;
}

#[tokio::test]
async fn test_auto_connect_mock() {
    test_auto_connect::<MockService>(make_mock_addr(), make_mock_addr()).await;
}

async fn connect_outbound_same_network<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::clone(&config)).await;
    let mut swarm2 = make_peer_manager::<T>(addr2, config).await;

    connect_services::<T>(
        &mut swarm1.peer_connectivity_handle,
        &mut swarm2.peer_connectivity_handle,
    )
    .await;
}

#[tokio::test]
async fn connect_outbound_same_network_libp2p() {
    connect_outbound_same_network::<Libp2pService>(make_libp2p_addr(), make_libp2p_addr()).await;
}

#[tokio::test]
async fn connect_outbound_same_network_mock() {
    connect_outbound_same_network::<MockService>(make_mock_addr(), make_mock_addr()).await;
}

#[tokio::test]
async fn test_validate_supported_protocols() {
    let config = Arc::new(config::create_mainnet());
    let swarm = make_peer_manager::<Libp2pService>(make_libp2p_addr(), config).await;

    // all needed protocols
    assert!(swarm.validate_supported_protocols(&default_protocols()));

    // all needed protocols + 2 extra
    assert!(swarm.validate_supported_protocols(
        &[
            Protocol::new(ProtocolType::PubSub, SemVer::new(1, 0, 0)),
            Protocol::new(ProtocolType::PubSub, SemVer::new(1, 1, 0)),
            Protocol::new(ProtocolType::Ping, SemVer::new(1, 0, 0)),
            Protocol::new(ProtocolType::Ping, SemVer::new(2, 0, 0)),
            Protocol::new(ProtocolType::Sync, SemVer::new(0, 1, 0)),
            Protocol::new(ProtocolType::Sync, SemVer::new(3, 1, 2)),
        ]
        .into_iter()
        .collect()
    ));

    // all needed protocols but wrong version for sync
    assert!(!swarm.validate_supported_protocols(
        &[
            Protocol::new(ProtocolType::PubSub, SemVer::new(1, 0, 0)),
            Protocol::new(ProtocolType::PubSub, SemVer::new(1, 1, 0)),
            Protocol::new(ProtocolType::Ping, SemVer::new(1, 0, 0)),
            Protocol::new(ProtocolType::Sync, SemVer::new(0, 2, 0)),
        ]
        .into_iter()
        .collect()
    ));

    // ping protocol missing
    assert!(!swarm.validate_supported_protocols(
        &[
            Protocol::new(ProtocolType::PubSub, SemVer::new(1, 0, 0)),
            Protocol::new(ProtocolType::PubSub, SemVer::new(1, 1, 0)),
            Protocol::new(ProtocolType::Sync, SemVer::new(0, 1, 0)),
        ]
        .into_iter()
        .collect()
    ));
}

async fn connect_outbound_different_network<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::clone(&config)).await;
    let mut swarm2 = make_peer_manager::<T>(
        addr2,
        Arc::new(common::chain::config::Builder::test_chain().magic_bytes([1, 2, 3, 4]).build()),
    )
    .await;

    let (_address, peer_info) = connect_services::<T>(
        &mut swarm2.peer_connectivity_handle,
        &mut swarm1.peer_connectivity_handle,
    )
    .await;
    assert_ne!(peer_info.magic_bytes, *config.magic_bytes());
}

#[tokio::test]
async fn connect_outbound_different_network_libp2p() {
    connect_outbound_different_network::<Libp2pService>(make_libp2p_addr(), make_libp2p_addr())
        .await;
}

#[tokio::test]
async fn connect_outbound_different_network_mock() {
    connect_outbound_different_network::<MockService>(make_mock_addr(), make_mock_addr()).await;
}

async fn connect_inbound_same_network<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::clone(&config)).await;
    let mut swarm2 = make_peer_manager::<T>(addr2, config).await;

    let (address, peer_info) = connect_services::<T>(
        &mut swarm1.peer_connectivity_handle,
        &mut swarm2.peer_connectivity_handle,
    )
    .await;
    assert_eq!(
        swarm2.accept_inbound_connection(address, peer_info).await,
        Ok(())
    );
}

#[tokio::test]
async fn connect_inbound_same_network_libp2p() {
    connect_inbound_same_network::<Libp2pService>(make_libp2p_addr(), make_libp2p_addr()).await;
}

#[tokio::test]
async fn connect_inbound_same_network_mock() {
    // TODO: fix protocol ids to work
    // connect_inbound_same_network::<MockService>(make_mock_addr(), make_mock_addr()).await;
}

async fn connect_inbound_different_network<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::new(config::create_mainnet())).await;
    let mut swarm2 = make_peer_manager::<T>(
        addr2,
        Arc::new(common::chain::config::Builder::test_chain().magic_bytes([1, 2, 3, 4]).build()),
    )
    .await;

    let (address, peer_info) = connect_services::<T>(
        &mut swarm1.peer_connectivity_handle,
        &mut swarm2.peer_connectivity_handle,
    )
    .await;

    assert_eq!(
        swarm2.accept_inbound_connection(address, peer_info).await,
        Err(P2pError::ProtocolError(ProtocolError::DifferentNetwork(
            [1, 2, 3, 4],
            *config::create_mainnet().magic_bytes(),
        )))
    );
}

#[tokio::test]
async fn connect_inbound_different_network_libp2p() {
    connect_inbound_different_network::<Libp2pService>(make_libp2p_addr(), make_libp2p_addr())
        .await;
}

#[tokio::test]
async fn connect_inbound_different_network_mock() {
    connect_inbound_different_network::<MockService>(make_mock_addr(), make_mock_addr()).await;
}

async fn remote_closes_connection<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::new(config::create_mainnet())).await;
    let mut swarm2 = make_peer_manager::<T>(addr2, Arc::new(config::create_mainnet())).await;

    let (_address, _peer_info) = connect_services::<T>(
        &mut swarm1.peer_connectivity_handle,
        &mut swarm2.peer_connectivity_handle,
    )
    .await;

    assert_eq!(
        swarm2
            .peer_connectivity_handle
            .disconnect(*swarm1.peer_connectivity_handle.peer_id())
            .await,
        Ok(())
    );
    assert!(std::matches!(
        swarm1.peer_connectivity_handle.poll_next().await,
        Ok(net::types::ConnectivityEvent::ConnectionClosed { .. })
    ));
}

#[tokio::test]
async fn remote_closes_connection_libp2p() {
    remote_closes_connection::<Libp2pService>(make_libp2p_addr(), make_libp2p_addr()).await;
}

#[tokio::test]
async fn remote_closes_connection_mock() {
    remote_closes_connection::<MockService>(make_mock_addr(), make_mock_addr()).await;
}

async fn inbound_connection_too_many_peers<T>(
    addr1: T::Address,
    addr2: T::Address,
    default_addr: T::Address,
    peers: Vec<net::types::PeerInfo<T>>,
) where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::clone(&config)).await;
    let mut swarm2 = make_peer_manager::<T>(addr2, Arc::clone(&config)).await;

    peers.into_iter().for_each(|info| {
        swarm1.peerdb.peer_connected(default_addr.clone(), info);
    });
    assert_eq!(
        swarm1.peerdb.active_peer_count(),
        peer_manager::MAX_ACTIVE_CONNECTIONS
    );

    let (_address, _peer_info) = connect_services::<T>(
        &mut swarm1.peer_connectivity_handle,
        &mut swarm2.peer_connectivity_handle,
    )
    .await;
    let swarm1_id = *swarm1.peer_connectivity_handle.peer_id();

    // run the first peer manager in the background and poll events from the peer manager
    // that tries to connect to the first manager
    tokio::spawn(async move { swarm1.run().await });

    if let Ok(net::types::ConnectivityEvent::ConnectionClosed { peer_id }) =
        swarm2.peer_connectivity_handle.poll_next().await
    {
        assert_eq!(peer_id, swarm1_id);
    } else {
        panic!("invalid event received");
    }
}

#[tokio::test]
async fn inbound_connection_too_many_peers_libp2p() {
    let config = Arc::new(config::create_mainnet());
    let peers = (0..peer_manager::MAX_ACTIVE_CONNECTIONS)
        .map(|_| net::types::PeerInfo {
            peer_id: PeerId::random(),
            magic_bytes: *config.magic_bytes(),
            version: common::primitives::semver::SemVer::new(0, 1, 0),
            agent: None,
            protocols: default_protocols(),
        })
        .collect::<Vec<_>>();

    inbound_connection_too_many_peers::<Libp2pService>(
        make_libp2p_addr(),
        make_libp2p_addr(),
        make_libp2p_addr(),
        peers,
    )
    .await;
}

#[tokio::test]
async fn inbound_connection_too_many_peers_mock() {
    let config = Arc::new(config::create_mainnet());
    let peers = (0..peer_manager::MAX_ACTIVE_CONNECTIONS)
        .map(|_| net::types::PeerInfo::<MockService> {
            peer_id: MockPeerId::random(),
            magic_bytes: *config.magic_bytes(),
            version: common::primitives::semver::SemVer::new(0, 1, 0),
            agent: None,
            protocols: [
                Protocol::new(ProtocolType::PubSub, SemVer::new(1, 0, 0)),
                Protocol::new(ProtocolType::Sync, SemVer::new(1, 0, 0)),
            ]
            .into_iter()
            .collect(),
        })
        .collect::<Vec<_>>();

    inbound_connection_too_many_peers::<MockService>(
        make_mock_addr(),
        make_mock_addr(),
        make_mock_addr(),
        peers,
    )
    .await;
}

async fn connection_timeout<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let mut swarm1 = make_peer_manager::<T>(addr1, Arc::clone(&config)).await;

    swarm1
        .peer_connectivity_handle
        .connect(addr2.clone())
        .await
        .expect("dial to succeed");

    match timeout(
        Duration::from_secs(swarm1._p2p_config.outbound_connection_timeout),
        swarm1.peer_connectivity_handle.poll_next(),
    )
    .await
    {
        Ok(res) => assert!(std::matches!(
            res,
            Ok(net::types::ConnectivityEvent::ConnectionError {
                address: _,
                error: P2pError::DialError(DialError::ConnectionRefusedOrTimedOut),
            })
        )),
        Err(_err) => panic!("did not receive `ConnectionError` in time"),
    }
}

#[tokio::test]
async fn connection_timeout_libp2p() {
    connection_timeout::<Libp2pService>(
        make_libp2p_addr(),
        format!("/ip4/255.255.255.255/tcp/8888/p2p/{}", PeerId::random())
            .parse()
            .unwrap(),
    )
    .await;
}

#[tokio::test]
#[ignore]
async fn connection_timeout_mock() {
    // TODO: implement timeouts for mock backend
}

// try to establish a new connection through RPC and verify that it is notified of the timeout
async fn connection_timeout_rpc_notified<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + 'static + std::fmt::Debug,
    T::ConnectivityHandle: ConnectivityService<T>,
    <T as net::NetworkingService>::Address: std::str::FromStr,
    <<T as net::NetworkingService>::Address as std::str::FromStr>::Err: std::fmt::Debug,
{
    let config = Arc::new(config::create_mainnet());
    let p2p_config = Arc::new(Default::default());
    let (conn, _, _) = T::start(addr1, Arc::clone(&config), Arc::clone(&p2p_config)).await.unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (tx_sync, mut rx_sync) = tokio::sync::mpsc::unbounded_channel();

    let mut swarm = peer_manager::PeerManager::<T>::new(
        Arc::clone(&config),
        Arc::clone(&p2p_config),
        conn,
        rx,
        tx_sync,
    );

    tokio::spawn(async move {
        loop {
            let _ = rx_sync.recv().await;
        }
    });
    tokio::spawn(async move {
        swarm.run().await.unwrap();
    });

    let (rtx, rrx) = oneshot::channel();
    tx.send(SwarmEvent::Connect(addr2, rtx)).unwrap();

    match timeout(
        Duration::from_secs(p2p_config.outbound_connection_timeout),
        rrx,
    )
    .await
    {
        Ok(res) => assert!(std::matches!(
            res.unwrap(),
            Err(P2pError::DialError(DialError::ConnectionRefusedOrTimedOut))
        )),
        Err(_err) => panic!("did not receive `ConnectionError` in time"),
    }
}

#[tokio::test]
async fn connection_timeout_rpc_notified_libp2p() {
    connection_timeout_rpc_notified::<Libp2pService>(
        make_libp2p_addr(),
        format!("/ip4/255.255.255.255/tcp/8888/p2p/{}", PeerId::random())
            .parse()
            .unwrap(),
    )
    .await;
}

#[tokio::test]
#[ignore]
async fn connection_timeout_rpc_notified_mock() {
    // TODO: implement timeouts for mock backend
}
