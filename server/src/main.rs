use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use cuda_over_ip_protocol::protocol::{NvmlReturnT, call, Call, NvmlInitWithFlagsCall, result, Result, NvmlInitWithFlagsResult};
use prost::Message;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use cuda_over_ip_protocol::protocol::call::Type;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:19999").unwrap();

    let (tcp_stream_read, _) = listener.accept().unwrap();

    tcp_stream_read.set_nodelay(true).unwrap();
    let tcp_stream_write = tcp_stream_read.try_clone().unwrap();
    let call = read_call(&tcp_stream_write);
    println!("{:?}", call);
    let result = handle_call(call);
    println!("{:?}", result);

    let mut buf = Vec::<u8>::with_capacity(result.encoded_len());
    result.encode(&mut buf).unwrap();
    let mut buf_writer = BufWriter::new(tcp_stream_write);
    buf_writer.write_u32::<BigEndian>(result.encoded_len() as u32).unwrap();
    buf_writer.write_all(&buf).unwrap();
    buf_writer.flush().unwrap();
}

fn read_call(tcp_stream_read: &TcpStream) -> Call {
    let mut buf_reader = BufReader::new(tcp_stream_read);
    let size = buf_reader.read_u32::<BigEndian>().unwrap() as usize;
    let mut buf = vec![0_u8; size];
    buf_reader.read_exact(&mut buf).unwrap();
    Call::decode(&*buf).unwrap()
}

fn handle_call(call: Call) -> Result {
    let type_ = call.r#type.expect("No type provided");
    match type_ {
        Type::NvmlInitWithFlagsCall(_) => {
            Result {
                r#type: Some(
                    result::Type::NvmlInitWithFlagsResult(NvmlInitWithFlagsResult {
                        r#return: NvmlReturnT::NvmlErrorUnknown as i32
                    })
                ),
            }
        }
    }
}
