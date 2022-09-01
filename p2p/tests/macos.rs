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

// #![allow(unused)]
// use chainstate::{chainstate_interface::ChainstateInterface, BlockSource};
// use common::{
//     chain::{config::ChainConfig, GenBlock},
//     primitives::{Id, Idable},
// };
// use p2p::{
//     error::P2pError,
//     event::{PubSubControlEvent, SwarmEvent, SyncControlEvent},
//     message::{BlockListRequest, BlockListResponse, HeaderListResponse, Request, Response},
//     net::{
//         self, libp2p::Libp2pService, mock::MockService, types::ConnectivityEvent,
//         ConnectivityService, NetworkingService, SyncingMessagingService,
//     },
//     sync::BlockSyncManager,
//     sync::SyncState,
// };
// use p2p_test_utils::{connect_services, make_libp2p_addr, make_mock_addr, TestBlockInfo};
// use std::net::SocketAddr;
// use std::{
//     collections::{HashSet, VecDeque},
//     sync::Arc,
// };
// use tokio::sync::mpsc;

// async fn make_sync_manager<T>(
//     addr: T::Address,
//     handle: subsystem::Handle<Box<dyn ChainstateInterface>>,
// ) -> (
//     BlockSyncManager<T>,
//     T::ConnectivityHandle,
//     mpsc::UnboundedSender<SyncControlEvent<T>>,
//     mpsc::UnboundedReceiver<PubSubControlEvent>,
//     mpsc::UnboundedReceiver<SwarmEvent<T>>,
// )
// where
//     T: NetworkingService,
//     T::ConnectivityHandle: ConnectivityService<T>,
//     T::SyncingMessagingHandle: SyncingMessagingService<T>,
// {
//     let (tx_p2p_sync, rx_p2p_sync) = mpsc::unbounded_channel();
//     let (tx_pubsub, rx_pubsub) = mpsc::unbounded_channel();
//     let (tx_swarm, rx_swarm) = mpsc::unbounded_channel();

//     let config = Arc::new(common::chain::config::create_mainnet());
//     let (conn, _, sync) = T::start(addr, Arc::clone(&config), Default::default()).await.unwrap();

//     (
//         BlockSyncManager::<T>::new(
//             Arc::clone(&config),
//             sync,
//             handle,
//             rx_p2p_sync,
//             tx_swarm,
//             tx_pubsub,
//         ),
//         conn,
//         tx_p2p_sync,
//         rx_pubsub,
//         rx_swarm,
//     )
// }

// // initialize two blockchains which have the same longest chain that is `num_blocks` long
// async fn init_chainstate_2(
//     config: Arc<ChainConfig>,
//     num_blocks: usize,
// ) -> (
//     subsystem::Handle<Box<dyn ChainstateInterface>>,
//     subsystem::Handle<Box<dyn ChainstateInterface>>,
// ) {
//     let handle1 = p2p_test_utils::start_chainstate(Arc::clone(&config)).await;
//     let handle2 = p2p_test_utils::start_chainstate(Arc::clone(&config)).await;
//     let blocks = p2p_test_utils::create_n_blocks(
//         Arc::clone(&config),
//         TestBlockInfo::from_genesis(config.genesis_block()),
//         num_blocks,
//     );

//     p2p_test_utils::import_blocks(&handle1, blocks.clone()).await;
//     p2p_test_utils::import_blocks(&handle2, blocks).await;

//     (handle1, handle2)
// }

// async fn get_tip(handle: &subsystem::Handle<Box<dyn ChainstateInterface>>) -> Id<GenBlock> {
//     handle.call(move |this| this.get_best_block_id()).await.unwrap().unwrap()
// }

// async fn same_tip(
//     handle1: &subsystem::Handle<Box<dyn ChainstateInterface>>,
//     handle2: &subsystem::Handle<Box<dyn ChainstateInterface>>,
// ) -> bool {
//     get_tip(handle1).await == get_tip(handle2).await
// }

// async fn advance_mgr_state<T>(mgr: &mut BlockSyncManager<T>) -> Result<(), P2pError>
// where
//     T: NetworkingService,
//     T::SyncingMessagingHandle: SyncingMessagingService<T>,
// {
//     match mgr.handle_mut().poll_next().await.unwrap() {
//         net::types::SyncingEvent::Request {
//             peer_id,
//             request_id,
//             request,
//         } => match request {
//             Request::HeaderListRequest(request) => {
//                 mgr.process_header_request(peer_id, request_id, request.into_locator()).await?;
//             }
//             Request::BlockListRequest(request) => {
//                 mgr.process_block_request(peer_id, request_id, request.into_block_ids()).await?;
//             }
//         },
//         net::types::SyncingEvent::Response {
//             peer_id,
//             request_id: _,
//             response,
//         } => match response {
//             Response::HeaderListResponse(response) => {
//                 mgr.process_header_response(peer_id, response.into_headers()).await?;
//             }
//             Response::BlockListResponse(response) => {
//                 mgr.process_block_response(peer_id, response.into_blocks()).await?;
//             }
//         },
//         net::types::SyncingEvent::Error {
//             peer_id,
//             request_id,
//             error,
//         } => {
//             mgr.process_error(peer_id, request_id, error).await?;
//         }
//     }

//     mgr.check_state().await
// }

// #[tokio::test]
// async fn macos_mre() {
//     let config = Arc::new(common::chain::config::create_unit_test_config());
//     let (handle1, handle2) = init_chainstate_2(Arc::clone(&config), 8).await;
//     let mgr1_handle = handle1.clone();
//     let mgr2_handle = handle2.clone();

//     let (mut mgr1, mut conn1, _, mut pubsub, _) =
//         make_sync_manager::<MockService>(make_mock_addr(), handle1).await;
//     let (mut mgr2, mut conn2, _, _, _) =
//         make_sync_manager::<MockService>(make_mock_addr(), handle2).await;

//     // add 14 more blocks to local chain and 7 more blocks to remote chain
//     assert!(same_tip(&mgr1_handle, &mgr2_handle).await);
//     p2p_test_utils::add_more_blocks(Arc::clone(&config), &mgr1_handle, 14).await;

//     assert!(!same_tip(&mgr1_handle, &mgr2_handle).await);
//     p2p_test_utils::add_more_blocks(Arc::clone(&config), &mgr2_handle, 7).await;

//     // save local and remote tips so we can verify who did a reorg
//     let local_tip = get_tip(&mgr1_handle).await;
//     let remote_tip = get_tip(&mgr2_handle).await;

//     // add peer to the hashmap of known peers and send getheaders request to them
//     connect_services::<MockService>(&mut conn1, &mut conn2).await;
//     assert_eq!(mgr1.register_peer(*conn2.peer_id()).await, Ok(()));
//     assert_eq!(mgr2.register_peer(*conn1.peer_id()).await, Ok(()));

//     let handle = tokio::spawn(async move {
//         for _ in 0..24 {
//             advance_mgr_state(&mut mgr1).await.unwrap();
//         }

//         mgr1
//     });

//     let mut work = VecDeque::new();

//     for _ in 0..24 {
//         match mgr2.handle_mut().poll_next().await.unwrap() {
//             net::types::SyncingEvent::Request {
//                 peer_id: _,
//                 request_id,
//                 request: Request::HeaderListRequest(request),
//             } => {
//                 let headers = mgr2_handle
//                     .call(move |this| this.get_headers(request.into_locator()))
//                     .await
//                     .unwrap()
//                     .unwrap();
//                 mgr2.handle_mut()
//                     .send_response(
//                         request_id,
//                         Response::HeaderListResponse(HeaderListResponse::new(headers)),
//                     )
//                     .await
//                     .unwrap()
//             }
//             net::types::SyncingEvent::Request {
//                 peer_id: _,
//                 request_id,
//                 request: Request::BlockListRequest(request),
//             } => {
//                 assert_eq!(request.block_ids().len(), 1);
//                 let id = request.block_ids()[0];
//                 let blocks = vec![mgr2_handle
//                     .call(move |this| this.get_block(id))
//                     .await
//                     .unwrap()
//                     .unwrap()
//                     .unwrap()];
//                 mgr2.handle_mut()
//                     .send_response(
//                         request_id,
//                         Response::BlockListResponse(BlockListResponse::new(blocks)),
//                     )
//                     .await
//                     .unwrap();
//             }
//             net::types::SyncingEvent::Response {
//                 peer_id,
//                 request_id: _,
//                 response: Response::BlockListResponse(response),
//             } => {
//                 assert_eq!(response.blocks().len(), 1);
//                 let block = response.blocks()[0].clone();
//                 mgr2_handle
//                     .call_mut(move |this| this.process_block(block, BlockSource::Peer))
//                     .await
//                     .unwrap()
//                     .unwrap();

//                 if let Some(header) = work.pop_front() {
//                     mgr2.handle_mut()
//                         .send_request(
//                             peer_id,
//                             Request::BlockListRequest(BlockListRequest::new(vec![header])),
//                         )
//                         .await
//                         .unwrap();
//                 }
//             }
//             net::types::SyncingEvent::Response {
//                 peer_id,
//                 request_id: _,
//                 response: Response::HeaderListResponse(response),
//             } => {
//                 let headers = mgr2_handle
//                     .call(move |this| this.filter_already_existing_blocks(response.into_headers()))
//                     .await
//                     .unwrap()
//                     .unwrap();
//                 work = headers.into_iter().map(|header| header.get_id()).collect::<VecDeque<_>>();
//                 let header = work.pop_front().unwrap();
//                 mgr2.handle_mut()
//                     .send_request(
//                         peer_id,
//                         Request::BlockListRequest(BlockListRequest::new(vec![header])),
//                     )
//                     .await
//                     .unwrap();
//             }
//             msg => panic!("invalid message received: {:?}", msg),
//         }
//     }

//     let mut mgr1 = handle.await.unwrap();
//     mgr1.check_state().await.unwrap();
//     mgr2.check_state().await.unwrap();

//     assert!(get_tip(&mgr1_handle).await == local_tip);
//     assert!(get_tip(&mgr2_handle).await != remote_tip);
//     assert_eq!(mgr1.state(), &SyncState::Idle);
//     assert_eq!(
//         pubsub.try_recv(),
//         Ok(PubSubControlEvent::InitialBlockDownloadDone),
//     );
// }
