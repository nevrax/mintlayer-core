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

use super::*;
use crate::{message::*, net::mock::MockService};
use p2p_test_utils::{make_libp2p_addr, make_mock_addr};
use std::{collections::HashSet, time::Duration};
use tokio::time::timeout;

async fn test_request_response<T>(addr1: T::Address, addr2: T::Address)
where
    T: NetworkingService + std::fmt::Debug + 'static,
    T::ConnectivityHandle: ConnectivityService<T>,
    T::SyncingMessagingHandle: SyncingMessagingService<T>,
{
    let (mut mgr1, mut conn1, _sync1, _pubsub1, _swarm1) = make_sync_manager::<T>(addr1).await;
    let (mut mgr2, mut conn2, _sync2, _pubsub2, _swarm2) = make_sync_manager::<T>(addr2).await;

    // connect the two managers together so that they can exchange messages
    connect_services::<T>(&mut conn1, &mut conn2).await;

    mgr1.peer_sync_handle
        .send_request(
            *conn2.peer_id(),
            Request::HeaderListRequest(HeaderListRequest::new(Locator::new(vec![]))),
        )
        .await
        .unwrap();

    if let Ok(net::types::SyncingEvent::Request {
        peer_id: _,
        request_id,
        request,
    }) = mgr2.peer_sync_handle.poll_next().await
    {
        assert_eq!(
            request,
            Request::HeaderListRequest(HeaderListRequest::new(Locator::new(vec![])))
        );

        mgr2.peer_sync_handle
            .send_response(
                request_id,
                Response::HeaderListResponse(HeaderListResponse::new(vec![])),
            )
            .await
            .unwrap();
    } else {
        panic!("invalid data received");
    }
}

#[tokio::test]
async fn test_request_response_libp2p() {
    test_request_response::<Libp2pService>(make_libp2p_addr(), make_libp2p_addr()).await;
}

// TODO: fix https://github.com/mintlayer/mintlayer-core/issues/375
#[tokio::test]
async fn test_request_response_mock() {
    test_request_response::<MockService>(make_mock_addr(), make_mock_addr()).await;
}

#[tokio::test]
async fn test_multiple_requests_and_responses() {
    let (mut mgr1, mut conn1, _sync1, _pubsub1, _swarm1) =
        make_sync_manager::<MockService>(make_mock_addr()).await;
    let (mut mgr2, mut conn2, _sync2, _pubsub2, _swarm2) =
        make_sync_manager::<MockService>(make_mock_addr()).await;
    // let config = Arc::new(common::chain::config::create_unit_test_config());
    // let (mut conn1, _, mut sync1) =
    //     MockService::start(make_mock_addr(), Arc::clone(&config), Default::default())
    //         .await
    //         .unwrap();

    // let (mut conn2, _, mut sync2) =
    //     MockService::start(make_mock_addr(), Arc::clone(&config), Default::default())
    //         .await
    //         .unwrap();

    // connect the two managers together so that they can exchange messages
    connect_services::<MockService>(&mut conn1, &mut conn2).await;
    let mut request_ids = HashSet::new();

    let id = mgr1
        .peer_sync_handle
        .send_request(
            *conn2.peer_id(),
            Request::HeaderListRequest(HeaderListRequest::new(Locator::new(vec![]))),
        )
        .await
        .unwrap();
    request_ids.insert(id);

    println!("first request sent");

    let id = mgr1
        .peer_sync_handle
        .send_request(
            *conn2.peer_id(),
            Request::HeaderListRequest(HeaderListRequest::new(Locator::new(vec![]))),
        )
        .await
        .unwrap();
    request_ids.insert(id);

    assert_eq!(request_ids.len(), 2);

    println!("second request sent");

    match timeout(Duration::from_secs(15), mgr2.peer_sync_handle.poll_next()).await {
        Ok(event) => match event {
            Ok(net::types::SyncingEvent::Request { request_id, .. }) => {
                println!("first request received");
                mgr2.peer_sync_handle
                    .send_response(
                        request_id,
                        Response::HeaderListResponse(HeaderListResponse::new(vec![])),
                    )
                    .await
                    .unwrap();
            }
            _ => panic!("invalid event: {:?}", event),
        },
        Err(_) => panic!("did not receive `Request` in time"),
    }

    println!("receive second request");

    match timeout(Duration::from_secs(15), mgr2.peer_sync_handle.poll_next()).await {
        Ok(event) => match event {
            Ok(net::types::SyncingEvent::Request { request_id, .. }) => {
                println!("second request received");
                mgr2.peer_sync_handle
                    .send_response(
                        request_id,
                        Response::HeaderListResponse(HeaderListResponse::new(vec![])),
                    )
                    .await
                    .unwrap();
            }
            _ => panic!("invalid event: {:?}", event),
        },
        Err(_) => panic!("did not receive `Request` in time"),
    }
}
