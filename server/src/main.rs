use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use cuda_over_ip_protocol::{protocol_calls, protocol_responses};
use prost::Message;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpListener;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:19999").unwrap();

    let (tcp_stream_read, _) = listener.accept().unwrap();
    tcp_stream_read.set_nodelay(true).unwrap();
    let tcp_stream_write = tcp_stream_read.try_clone().unwrap();
    {
        let mut buf_reader = BufReader::new(tcp_stream_read);
        let size = buf_reader.read_u32::<BigEndian>().unwrap() as usize;
        let mut buf = vec![0_u8; size];
        buf_reader.read_exact(&mut buf).unwrap();
        let call = protocol_calls::RemoteCall::decode(&*buf).unwrap();
        println!("{:?}", call);
    }

    {
        let response = protocol_responses::RemoteResponse {
            call: Some(protocol_responses::remote_response::Call::NvmlInitWithFlags(
                protocol_responses::NvmlInitWithFlags { result: 0 }
            ))
        };
        println!("{:?}", response);
        let mut buf = Vec::<u8>::with_capacity(response.encoded_len());
        response.encode(&mut buf).unwrap();

        let mut buf_writer = BufWriter::new(tcp_stream_write);
        buf_writer.write_u32::<BigEndian>(response.encoded_len() as u32).unwrap();
        buf_writer.write_all(&buf).unwrap();
        buf_writer.flush().unwrap();
    }
}
