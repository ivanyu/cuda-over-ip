use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;
use std::ops::Deref;
use std::process::exit;
use std::sync::Mutex;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use prost::Message;
use static_init::dynamic;
use cuda_over_ip_protocol::protocol::{FuncCall, FuncResult};

#[dynamic(drop)]
pub(crate) static mut WRITER_AND_READER: Mutex<(BufWriter<TcpStream>, BufReader<TcpStream>)> = {
    let read_stream = match TcpStream::connect("127.0.0.1:19999") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error connecting to server: {}", e);
            exit(1);
        }
    };
    read_stream.set_nodelay(true).unwrap();
    let write_stream = match read_stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error connecting to server: {}", e);
            exit(1);
        }
    };

    Mutex::new((BufWriter::new(write_stream), BufReader::new(read_stream)))
};

pub(crate) fn send_call_and_get_result(buf_writer: &mut BufWriter<TcpStream>,
                                       buf_reader: &mut BufReader<TcpStream>,
                                       call: FuncCall) -> FuncResult {
    send_call(buf_writer, call);
    read_result(buf_reader)
}

fn send_call(buf_writer: &mut BufWriter<TcpStream>, call: FuncCall) {
    fn int(buf_writer: &mut BufWriter<TcpStream>, call: FuncCall) -> std::io::Result<()> {
        let mut buf = Vec::<u8>::with_capacity(call.encoded_len());
        call.encode(&mut buf)?;

        buf_writer.write_u32::<BigEndian>(call.encoded_len() as u32)?;
        buf_writer.write_all(&buf)?;
        buf_writer.flush()
    }

    if let Err(e) = int(buf_writer, call) {
        eprintln!("Error sending call: {}", e);
        exit(1);
    };
}

fn read_result(buf_reader: &mut BufReader<TcpStream>) -> FuncResult {
    fn int(buf_reader: &mut BufReader<TcpStream>) -> std::io::Result<Vec<u8>> {
        let size = buf_reader.read_u32::<BigEndian>()? as usize;
        let mut buf = vec![0_u8; size];
        buf_reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    match int(buf_reader) {
        Ok(buf) => {
            match FuncResult::decode(buf.deref()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error decoding result: {}", e);
                    exit(1);
                }
            }
        }

        Err(e) => {
            eprintln!("Error reading result: {}", e);
            exit(1);
        }
    }
}
