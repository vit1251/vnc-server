
use std::io::Cursor;
use byteorder::{LittleEndian, BigEndian, NativeEndian, ReadBytesExt};
use tokio::time::Duration;
use tokio::time::sleep_until;
use tokio::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::error::Error;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
//use std::sync::Arc;
use tokio::sync::mpsc::{channel, Sender};

#[derive(Debug)]
enum Encoding {
    Raw,                                  // 0
    CopyRect,                             // 1
    RRE,                                  // 2
    Hextile,                              // 5
    TRLE,                                 // 15
    ZRLE,                                 // 16
    CursorPseudoEncoding,                 // -239
    DesktopSizePseudoEncoding,            // -223
    Custom(u32),                          // Custom
}

impl From<u32> for Encoding {
    fn from(value: u32) -> Self {
        match value {
            0 => Encoding::Raw,
            1 => Encoding::CopyRect,
            2 => Encoding::RRE,
            5 => Encoding::Hextile,
            15 => Encoding::TRLE,
            16 => Encoding::ZRLE,
            _ => Encoding::Custom(value),
        }
    }
}

enum Key {
    BackSpace          = 0xff08,
    Tab                = 0xff09,
    Enter              = 0xff0d,
    Escape             = 0xff1b,
    Insert             = 0xff63,
    Delete             = 0xffff,
    Home               = 0xff50,
    End                = 0xff57,
    PageUp             = 0xff55,
    PageDown           = 0xff56,
    Left               = 0xff51,
    Up                 = 0xff52,
    Right              = 0xff53,
    Down               = 0xff54,
    F1                 = 0xffbe,
    F2                 = 0xffbf,
    F3                 = 0xffc0,
    F4                 = 0xffc1,
    // ...
    F12                = 0xffc9,
    ShiftLeft          = 0xffe1,
    ShiftRight         = 0xffe2,
    ControlLeft        = 0xffe3,
    ControlRight       = 0xffe4,
    MetaLeft           = 0xffe7,
    MetaRight          = 0xffe8,
    AltLeft            = 0xffe9,
    AltRight           = 0xffea,
}


#[derive(Debug)]
enum ServerMessage {
}

#[derive(Debug)]
enum ClientMessage {
    Unknown,                                                                                         // -
    SetPixelFormat,                                                                                  // 1
    SetEncodings { encodings: Vec<Encoding> },                                                       // 2
    FramebufferUpdateRequest { incremental: bool, pos_x: u16, pos_y: u16, width: u16, height: u16 }, // 3
    KeyEvent { down_flag: bool, key: u32 },                                                          // 4
    PointerEvent { pos_x: u16, pos_y: u16 },                                                         // 5
    ClientCutText,                                                                                   // 6
}


pub struct RfbServerBuilder {
    host: String,
    port: u16,
    width: u16,
    height: u16,
    depth: u8,
}

pub struct RfbServer {
    width: u16,
    height: u16,
    depth: u8,
}


impl RfbServerBuilder {
    pub fn new() -> Self {
        Self {
    	    host: String::from("0.0.0.0"),
    	    port: 0,
    	    width: 640,
    	    height: 480,
    	    depth: 32
    	}
    }

    // Установка порта
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn build(self) -> RfbServer {
	RfbServer::new(self.width, self.height, self.depth)
    }

}


async fn framebufferUpdate(incremental: bool, writer: &mut OwnedWriteHalf) -> Result<(), Box<dyn Error>> {

    // Заливка серым (пример кадра)
    let width: u16 = 800;
    let height: u16 = 600;
    let fb_size = width as usize * height as usize * 4;
    let framebuffer = vec![0xCC; fb_size];
    let num_rects: u16 = 1;
    let num_mem = num_rects.to_be_bytes();
    // Прямоугольник 1
    let width_mem = width.to_be_bytes();
    let height_mem = height.to_be_bytes();

    if incremental {
        print!(".");
        writer.write_all(&[
            0,                                           // message-type
            0,                                           // padding
            0, 0,                                        // number-of-rectangles
        ]).await?;
        writer.flush().await?;
        return Ok(());
    }
    print!("!");

    // [1 байт: тип=0][1 байт: padding][2 байт: кол-во прямоугольников]
    writer.write_all(&[
        0,                                           // message-type
        0,                                           // padding
        num_mem[0], num_mem[1],                      // Число прямоугольников (2 байта, big-endian)
        0, 0,                                        // x (2 байта)
        0, 0,                                        // y (2 байта)
        width_mem[0], width_mem[1],                  // width (2 байта)
        height_mem[0], height_mem[1],                // height (2 байта)
        0, 0, 0, 0,                                  // Encoding type (4 байта, 0x00000000 для Raw)
    ]).await?;

    writer.write_all(&framebuffer).await?;
    writer.flush().await?;

    Ok(())
}

async fn handle_version(reader: &mut OwnedReadHalf, writer: &mut OwnedWriteHalf) -> Result<(), Box<dyn Error>> {

    // 1. Отправляем версию протокола сервера
    writer.write_all(b"RFB 003.008\n").await?;
    writer.flush().await?;

    // 2. Получаем версию протокола клиента
    let mut ver_buf = [0u8; 12];
    reader.read_exact(&mut ver_buf).await?;
    // в целом наплевать

    Ok(())
}

async fn handle_auth(reader: &mut OwnedReadHalf, writer: &mut OwnedWriteHalf) -> Result<(), Box<dyn Error>> {

    // [1 байт: количество типов][n байт: сами типы]
    // Типы: 1 = None, 2 = VNC Auth, 16 = TLS, др.
    writer.write_all(&[2, 1, 2]).await?;

    //
    let mut auth_buf = [0u8; 1];
    reader.read_exact(&mut auth_buf).await?;
    println!("Получен код авторизации: {:?}", auth_buf);
    if auth_buf[0] == 1 {
        writer.write_all(&[0, 0, 0, 0]).await?;  // OK
        writer.flush().await?;
        return Ok(())
    }

    if auth_buf[0] == 2 {

        // Отправляем 16‑байтный "challenge" (в реальном сервере — случайный)
        let challenge: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10
        ];
        writer.write_all(&challenge).await?;

        // Получаем ответ клиента (16 байт)
        let mut response = [0u8; 16];
        reader.read_exact(&mut response).await?;
        // в целом наплевать

        // 4. Отправляем успех проверки пароля
        writer.write_all(&[0, 0, 0, 0]).await?;  // OK
        writer.flush().await?;
        return Ok(())
    }

    println!("У нас нет поддержки такой авторизации!");

    Ok(())
}

async fn clientInit(reader: &mut OwnedReadHalf, writer: &mut OwnedWriteHalf) -> Result<(), Box<dyn Error>> {

    let mut share_flag = [0u8; 1];
    reader.read_exact(&mut share_flag).await?;
    // в целом как обычно

    Ok(())
}

async fn serverInit(reader: &mut OwnedReadHalf, writer: &mut OwnedWriteHalf) -> Result<(), Box<dyn Error>> {

    let width: u16 = 800;
    let height: u16 = 600;
    let width_mem = width.to_be_bytes();
    let height_mem = height.to_be_bytes();

    writer.write_all(&[
        width_mem[0], width_mem[1],           // framebuffer-width
        height_mem[0], height_mem[1],         // framebuffer-height
        32,                                   // bits-per-pixel
        24,                                   // depth
        0,                                    // big-endian-flag
        1,                                    // true-colour-flag
        0, 255,                               // red-max
        0, 255,                               // green-max
        0, 255,                               // blue-max
        16,                                   // red-shift
        8,                                    // green-shift
        0,                                    // blue-shift
        0, 0, 0,                              // padding
        0, 0, 0, 0                            // display name
    ]).await?;

    writer.flush().await?;

    Ok(())

}

async fn process(data: &mut Vec<u8>, reader_tx: &Sender<ClientMessage>) -> Result<(), Box<dyn Error>> {

    let mut cursor = Cursor::new(&data); // или Cursor::new(data.as_slice())
    let msg_type: u8 = ReadBytesExt::read_u8(&mut cursor)?;

//    println!("Поступил пакет: msg-type = {}", msg_type);

    match msg_type {
        0 => { // SetPixelFormat (0)
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;
            data.drain(..20);
        },
        2 => { // SetEncodings (2)
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;
            let number_of_encodings: u16 = ReadBytesExt::read_u16::<BigEndian>(&mut cursor)?;
            let mut encodings: Vec<Encoding> = Vec::new();
            for i in 0..number_of_encodings {
                let encoding_type: u32 = ReadBytesExt::read_u32::<BigEndian>(&mut cursor)?;
                let encoding: Encoding = Encoding::from(encoding_type);
                encodings.push(encoding);
            }
            reader_tx.send(ClientMessage::SetEncodings { encodings }).await?;
            let size: usize = cursor.position() as usize;
            data.drain(..size);
        },
        3 => { // FramebufferUpdateRequest (3)
            let incremental: u8 = ReadBytesExt::read_u8(&mut cursor)?;
            let pos_x: u16 = ReadBytesExt::read_u16::<BigEndian>(&mut cursor)?;
            let pos_y: u16 = ReadBytesExt::read_u16::<BigEndian>(&mut cursor)?;
            let width: u16 = ReadBytesExt::read_u16::<BigEndian>(&mut cursor)?;
            let height: u16 = ReadBytesExt::read_u16::<BigEndian>(&mut cursor)?;
            reader_tx.send(ClientMessage::FramebufferUpdateRequest {
                incremental: incremental == 1,
                pos_x: pos_x,
                pos_y: pos_y,
                width: width,
                height: height,
            }).await?;
            let size: usize = cursor.position() as usize;
            data.drain(..size);
        },
        4 => { // KeyEvent (4)
            let down_flag: u8 = ReadBytesExt::read_u8(&mut cursor)?;             // down-flag
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;                     // padding
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;                     // padding
            let key: u32 = ReadBytesExt::read_u32::<BigEndian>(&mut cursor)?;    // key
            reader_tx.send(ClientMessage::KeyEvent {
                down_flag: down_flag == 1,
                key: key,
            }).await?;
            let size: usize = cursor.position() as usize;
            data.drain(..size);
        },
        5 => { // PointerEvent (5)
            let button_mask: u8 = ReadBytesExt::read_u8(&mut cursor)?;                      // button-mask
            let pos_x: u16 = ReadBytesExt::read_u16::<BigEndian>(&mut cursor)?;             // x-position
            let pos_y: u16 = ReadBytesExt::read_u16::<BigEndian>(&mut cursor)?;             // y-position
            reader_tx.send(ClientMessage::PointerEvent {
                pos_x: pos_x,
                pos_y: pos_y,
            }).await?;
            let size: usize = cursor.position() as usize;
            data.drain(..size);
        },
        6 => { // ClientCutText (6)
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;                           // padding
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;                           // padding
            let _: u8 = ReadBytesExt::read_u8(&mut cursor)?;                           // padding
            let length: u32 = ReadBytesExt::read_u32::<BigEndian>(&mut cursor)?;       // length
            let size: usize = length as usize;
            let mut text = vec![0u8; size];
            cursor.read_exact(&mut text).await?;
            println!("ClientCutText: text = {:?}", text);
            reader_tx.send(ClientMessage::ClientCutText).await;
            let size: usize = cursor.position() as usize;
            data.drain(..size);
        },
        _ => {
            reader_tx.send(ClientMessage::Unknown).await;
        }
    };

    Ok(())
}

async fn handle_frame(deadline: Option<Instant>) -> bool {
    match deadline {
        Some(d) => {
            sleep_until(d).await;
            true // Фрейм нужно отправить
        }
        None => {
            // Бесконечно ждем - эта ветка никогда не завершится сама
            std::future::pending::<()>().await;
            false
        }
    }
}

async fn handle_client(stream: TcpStream) -> Result<(), Box<dyn Error>> {

    let (mut reader, mut writer) = stream.into_split();

    // 1. Фаза Handshake (рукопожатие)
    handle_version(&mut reader, &mut writer).await?;
    handle_auth(&mut reader, &mut writer).await?;

    // 2. Фаза Инициализации
    clientInit(&mut reader, &mut writer).await?;
    serverInit(&mut reader, &mut writer).await?;

    // 3. Фаза основной работы
    let mut last_event = Instant::now();                                    // Время получения последнего события
    let mut last_send = Instant::now();                                     // Вермя последней отправки изображения
    let mut next_frame: Option<Instant> = None;                             // Дэдлайн следующего кадра
    let (mut reader_tx, mut reader_rx) = channel::<ClientMessage>(4096);
    let (mut writer_tx, mut writer_rx) = channel(4096);

    tokio::spawn(async move {
        let mut data: Vec<u8> = Vec::new();
        let mut buf: [u8; 8192] = [0; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => {
                    reader_tx.send(ClientMessage::Unknown).await;
                    break;
                },
                Ok(size) => {
//                    println!("прочитано: size = {:?} buf = {:?}", size, &buf[..size]);
                    data.extend_from_slice(&buf[..size]);
//                    println!("сейчас данных: data = {:?}", data.len());
                    loop {
                        if let Err(_) = process(&mut data, &reader_tx).await {
                            break;
                        }
                    }

                },
                Err(_) => {
                    reader_tx.send(ClientMessage::Unknown).await;
                    break;
                }
            }
        }
    });

    tokio::spawn(async move {
        loop {
            match writer_rx.recv().await {
                Some(msg) => {
//                    println!("Пишем: msg = {:?}", msg);
                    match framebufferUpdate(msg, &mut writer).await {
                        Ok(_) => {},
                        Err(_) => break,
                    }
                },
                None => break,
            }
        }
    });

    loop {
        tokio::select! {
            // Читаем сообщения от клиента
            data = reader_rx.recv() => {
                match data {
                    Some(ClientMessage::Unknown) => {
                        println!("Нарушение протокола!");
                        break Ok(());
                    },
                    Some(ClientMessage::KeyEvent { key, .. }) => {
                        println!("нажата клавиша = {:?}", key);
                        last_event = Instant::now();
                    },
                    Some(ClientMessage::PointerEvent { pos_x, pos_y, .. }) => {
                        println!("мышка = {}x{}", pos_x, pos_y);
                        last_event = Instant::now();
                    },
                    Some(ClientMessage::FramebufferUpdateRequest { incremental, .. }) => {
                        next_frame = Some(Instant::now() + Duration::from_millis(1_000));
                    },
                    _ => {},
                }
            }
            // Если пользователь заснул - отправить BELL
            _ = handle_frame(next_frame) => {
                println!("Запрос на обновление!");
                let incremental: bool = false;
                writer_tx.send(incremental).await;
                next_frame = None;
                last_send = Instant::now();
            }
        }
    }
}


impl RfbServer {
    fn new(width: u16, height: u16, depth: u8) -> Self {
	Self {
	    width,
	    height,
	    depth
	}
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {

        let listener = TcpListener::bind("127.0.0.1:5900").await?;
        println!("VNC server listening on :5900 (TCP)");

        loop {
            let (stream, _) = listener.accept().await?;
            tokio::spawn(async move {
                if let Err(e) = handle_client(stream).await {
                    eprintln!("Client error: {}", e);
                }
            });
        }

//        Ok(())
    }
}
