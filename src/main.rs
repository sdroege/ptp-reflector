/*
 * Copyright (C) 2015 Sebastian Dröge <sebastian@centricular.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Library General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Library General Public License for more details.
 *
 * You should have received a copy of the GNU Library General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 51 Franklin St, Fifth Floor,
 * Boston, MA 02110-1301, USA.
 */

extern crate bytes;
extern crate mio;

use bytes::{ByteBuf};
use mio::*;
use mio::udp::*;
use std::net::{SocketAddr};
use std::ops::{Range};

const SOCKET_EVENT  : Token = Token(0);
const SOCKET_GENERAL: Token = Token(1);

struct PtpReflectorHandler {
    event_socket   : UdpSocket,
    event_addr     : SocketAddr,
    general_socket : UdpSocket,
    general_addr   : SocketAddr,
}

fn vector_map_range_in_place<T,F>(v: &mut Vec<T>, range: Range<usize>, mut f: F)
  where F: FnMut(&T) -> T {
    let n = v.len();

    if range.end > n {
        panic!("Invalid range {} > {}", range.end, n);
    }

    for i in range {
        v[i] = f(&v[i]);
    }
}

impl PtpReflectorHandler {
    fn new(event_loop: &mut EventLoop<PtpReflectorHandler>) -> Result<PtpReflectorHandler, String> {
        let multicast_group = "224.0.1.129".parse().unwrap();

        let event_bind_addr = "0.0.0.0:319".parse().unwrap();
        let event_socket = try!(UdpSocket::bound(&event_bind_addr).map_err(|e| e.to_string()));
        try!(event_socket.join_multicast(&multicast_group).map_err(|e|e.to_string()));
        try!(event_loop.register_opt(&event_socket, SOCKET_EVENT, EventSet::readable(), PollOpt::level()).map_err(|e| e.to_string()));

        // FIXME: How can we create this from the multicast group?
        //let event_addr = SocketAddr::new(multicast_group, 319);
        let event_addr = "224.0.1.129:319".parse().unwrap();

        let general_bind_addr = "0.0.0.0:320".parse().unwrap();
        let general_socket = try!(UdpSocket::bound(&general_bind_addr).map_err(|e| e.to_string()));
        try!(general_socket.join_multicast(&multicast_group).map_err(|e|e.to_string()));
        try!(event_loop.register_opt(&general_socket, SOCKET_GENERAL, EventSet::readable(), PollOpt::level()).map_err(|e| e.to_string()));

        // FIXME: How can we create this from the multicast group?
        //let general_addr = SocketAddr::new(multicast_group, 320);
        let general_addr = "224.0.1.129:320".parse().unwrap();

        Ok (PtpReflectorHandler {
            event_socket: event_socket,
            event_addr: event_addr,
            general_socket: general_socket,
            general_addr: general_addr,
        })
    }
}

impl Handler for PtpReflectorHandler {
    type Timeout = usize;
    type Message = ();

    fn ready(&mut self, _: &mut EventLoop<PtpReflectorHandler>, token: Token, events: EventSet) {
        if !events.is_readable() {
            return;
        }

        let (socket, addr) = match token {
            SOCKET_EVENT   => (&self.event_socket, &self.event_addr),
            SOCKET_GENERAL => (&self.general_socket, &self.general_addr),
            _              => panic!("unexpected token")
        };

        let mut recv_buf = ByteBuf::mut_with_capacity(1024);

        match socket.recv_from(&mut recv_buf) {
            Err(e) => {
                print!("Error while receiving message: {}\n", e.to_string());
                return;
            },
            _      => ()
        };

        let mut msg = recv_buf.flip().bytes().to_owned();

        print!("Got message of size {}\n", msg.len());

        if msg.len() < 44 {
            return;
        }

        let domain = msg[4];
        let msg_type = msg[0] & 0x0f;

        let forward = match domain {
            0 => {
                msg[4] = 1;
                vector_map_range_in_place(&mut msg, 20..28, |&x| x ^ 0xff);

                match msg_type {
                    0x0 | 0x08 | 0xb       => true,
                    0x9 if msg.len() >= 54 => {
                        vector_map_range_in_place(&mut msg, 44..52, |&x| x ^ 0xff);

                        true
                    },
                    _                      => false
                }
            },
            1 => {
                msg[4] = 0;
                vector_map_range_in_place(&mut msg, 20..28, |&x| x ^ 0xff);

                match msg_type {
                    0x1 => true,
                    _   => false
                }
            },
            _ => false
        };

        if forward {
            let mut send_buf = ByteBuf::from_slice(&msg);
            match socket.send_to(&mut send_buf, addr) {
                Err(e) => {
                    print!("Error while sending message: {}\n", e.to_string());
                    return;
                },
                _      => ()
            };

            print!("Sent message of size {}\n", msg.len());
        }
    }
}

fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let res =
        PtpReflectorHandler::new(&mut event_loop)
            .map_err(|e| e.to_string())
            .and_then(|mut handler| {
                event_loop.run(&mut handler)
                    .map_err(|e| e.to_string())
        });

    match res {
        Ok(_)  => (),
        Err(s) => panic!(s)
    };
}
