mod generated;

use crate::generated::handle_call;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use cuda_over_ip_protocol::protocol::{FuncCall, FuncResult};
use libloading::Library;
use prost::Message;
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:19999").unwrap();

    while let Ok((tcp_stream_read, _)) = listener.accept() {
        tcp_stream_read.set_nodelay(true).unwrap();
        thread::spawn(move || {
            serve(tcp_stream_read);
        });
    }
}

fn serve(tcp_stream_read: TcpStream) {
    let libnvidia = unsafe { Library::new("libnvidia-ml.so.1").unwrap() };

    let tcp_stream_write = tcp_stream_read.try_clone().unwrap();
    let mut buf_writer: BufWriter<TcpStream> = BufWriter::new(tcp_stream_write);
    let mut buf_reader: BufReader<TcpStream> = BufReader::new(tcp_stream_read);

    loop {
        let call = match read_call(&mut buf_reader) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading call: {}", e);
                break;
            }
        };
        println!("{:?}", call);

        let result = match handle_call(call, &libnvidia) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error handling call: {}", e);
                break;
            }
        };
        println!("{:?}", result);

        if let Err(e) = respond(&mut buf_writer, result) {
            eprintln!("Error responding: {}", e);
            break;
        }
    }
}

fn read_call(buf_reader: &mut BufReader<TcpStream>) -> std::io::Result<FuncCall> {
    let size = buf_reader.read_u32::<BigEndian>()? as usize;
    let mut buf = vec![0_u8; size];
    buf_reader.read_exact(&mut buf)?;
    Ok(FuncCall::decode(&*buf)?)
}

fn respond(buf_writer: &mut BufWriter<TcpStream>, result: FuncResult) -> std::io::Result<()> {
    let mut buf = Vec::<u8>::with_capacity(result.encoded_len());
    result.encode(&mut buf)?;
    buf_writer.write_u32::<BigEndian>(result.encoded_len() as u32)?;
    buf_writer.write_all(&buf)?;
    buf_writer.flush()
}
