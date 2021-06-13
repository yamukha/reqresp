
use async_trait::async_trait;
use libp2p_core::{
    Multiaddr,
    PeerId,
    identity,
    muxing::StreamMuxerBox,
    transport::{self, Transport},
    upgrade::{self, read_one, write_one}
};
use libp2p_noise::{NoiseConfig, X25519Spec, Keypair};
use libp2p_request_response::*;
use libp2p_swarm::{Swarm, SwarmEvent};
use libp2p_tcp::TcpConfig;
use futures::{prelude::*};
use std::{io, iter};
use reqresp::{*};
use std::{thread, time};
use std::env;
use rust_base58::{FromBase58};

/// Exercises a simple request response protocol.

fn main() {

    let protocols = iter::once((ReqRespProtocol(), ProtocolSupport::Full));
    let cfg = RequestResponseConfig::default();

    let (peer1_id, trans) = mk_transport();
    let ping_proto1 = RequestResponse::new(ReqRespCodec(), protocols.clone(), cfg.clone());
    let mut swarm1 = Swarm::new(trans, ping_proto1, peer1_id);

    let addr : Multiaddr = "/ip4/127.0.0.1/tcp/61241".parse().unwrap();
    swarm1.listen_on(addr.clone()).unwrap();
    let ms = time::Duration::from_millis(1000);

    let peer1 = async move {
         println!("Local peer 1 id: {:?}", peer1_id);
        loop {
            match swarm1.next_event().await {
                SwarmEvent::NewListenAddr(addr) => {
                   println!("Peer 1 listening on {}", addr.clone());
                },

                SwarmEvent::Behaviour(RequestResponseEvent::Message {
                    peer,
                    message: RequestResponseMessage::Request { request, channel, .. }
                }) => {

                    let req = String::from_utf8_lossy(&request.0);
                    println!(" peer1 Req -> {:?} to {:?}", req.clone(), peer);
                   
                    if  req == "quit" {
                        thread::sleep(ms);
                        return
                    }
                    let resp = Resp(format!("Hello {}", req).into_bytes());
                    println!(" peer1 Resp : {:?}", String::from_utf8_lossy(&resp.0));
                    swarm1.behaviour_mut().send_response(channel, resp.clone()).unwrap();
                },

                SwarmEvent::Behaviour(RequestResponseEvent::ResponseSent {
                    peer, ..
                }) => {
                     println!("Response sent to peer ID {:?}",  peer);
                }

                SwarmEvent::Behaviour(e) => panic!("Peer1: Unexpected event: {:?}", e),
                _ => {}
            }
        }
    };

    let args: Vec<String> = env::args().collect();

    let num_pings: u32 = 3;

    let peer2 = async move {

       let peer_id = std::env::args().nth(1).unwrap();
       let remote_bytes = peer_id.from_base58().unwrap(); 
       let remote_peer = PeerId::from_bytes(&remote_bytes).unwrap();

       let (peer2_id, trans) = mk_transport();
       let ping_proto2 = RequestResponse::new(ReqRespCodec(), protocols, cfg);
       let mut swarm2 = Swarm::new(trans, ping_proto2, peer2_id.clone());

        let mut count = 0;
        println!("Peer 2 get addr {}", addr.clone());
        println!("Local peer 2 id: {:?}", peer2_id);
        swarm2.behaviour_mut().add_address(&remote_peer, addr.clone());

        let mut rq = Req(count.to_string().into_bytes());
        let mut req_id = swarm2.behaviour_mut().send_request(&remote_peer, rq.clone());
        println!(" peer2 Req  ID {} -> {:?} : {}", req_id, remote_peer,String::from_utf8_lossy(&rq.0));

        loop {
            match swarm2.next().await {
                RequestResponseEvent::Message {
                    peer,
                    message: RequestResponseMessage::Response { request_id, response }
                } => {
                    count += 1;
                    println!(" peer2 Resp ID {}  : {} to {:?}", request_id, String::from_utf8_lossy(&response.0), peer);
                   if count >= num_pings {
                        rq = Req("quit".to_string().into_bytes());
                        req_id = swarm2.behaviour_mut().send_request(&remote_peer, rq.clone());
                        println!(" peer2 Req  ID {} -> {:?} : {}", req_id, remote_peer,String::from_utf8_lossy(&rq.0));
                        return
                    } else {
                        rq = Req(count.to_string().into_bytes());
                        req_id = swarm2.behaviour_mut().send_request(&remote_peer, rq.clone());
                        println!(" peer2 Req  ID {} -> {:?} : {}", req_id, remote_peer,String::from_utf8_lossy(&rq.0));
                    }
                },
                e => panic!("Peer2: Unexpected event: {:?}", e)
            }
        }
    };

    if  args.len() < 2 {
        let () = async_std::task::block_on(peer1);
    } else {
        let () = async_std::task::block_on(peer2);
    }
}

fn mk_transport() -> (PeerId, transport::Boxed<(PeerId, StreamMuxerBox)>) {
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = id_keys.public().into_peer_id();
    let noise_keys = Keypair::<X25519Spec>::new().into_authentic(&id_keys).unwrap();
    (peer_id, TcpConfig::new()
        .nodelay(true)
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(libp2p_yamux::YamuxConfig::default())
        .boxed())
}

#[derive(Clone)]
pub struct ReqRespCodec();

#[derive(Debug, Clone)]
pub struct ReqRespProtocol();

impl ProtocolName for ReqRespProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/reqresp/1".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for ReqRespCodec {
    type Protocol = ReqRespProtocol;
    type Request = Req;
    type Response = Resp;

    async fn read_request<T>(&mut self, _: &ReqRespProtocol, io: &mut T)
        -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send
    {
        read_one(io, 1024)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(vec) => Ok(Req(vec))
            })
            .await
    }

    async fn read_response<T>(&mut self, _: &ReqRespProtocol, io: &mut T)
        -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send {
        read_one(io, 1024)
            .map(|res| match res {
                Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
                Ok(vec) if vec.is_empty() => Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(vec) => Ok(Resp(vec))
            })
            .await
    }

    async fn write_request<T>(&mut self, _: &ReqRespProtocol, io: &mut T, Req(data): Req)
        -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send {
        write_one(io, data).await
    }

    async fn write_response<T>(&mut self, _: &ReqRespProtocol, io: &mut T, Resp(data): Resp)
        -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send {
        write_one(io, data).await
    }
}
