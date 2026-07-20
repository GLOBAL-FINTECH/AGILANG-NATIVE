pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod documents;
pub mod formatting;
pub mod hover;
pub mod protocol;
pub mod server;
pub mod symbols;
pub mod transport;

pub use server::run_lsp_server;

#[cfg(test)]
mod tests {
    use super::*;
    use diagnostics::{offset_to_position, position_to_offset};
    use documents::{path_to_uri, uri_to_path};
    use std::io::Cursor;
    use transport::read_framed_message;

    #[test]
    fn test_uri_to_path_roundtrip() {
        let uri1 = "file:///C:/Users/user/app/main.agi";
        let path1 = uri_to_path(uri1).unwrap();
        assert_eq!(
            path1.to_str().unwrap().replace('\\', "/"),
            "C:/Users/user/app/main.agi"
        );
        assert_eq!(path_to_uri(&path1), uri1);

        let uri2 = "file:///W:/agilang-native-runtime/examples/native/hello.agi";
        let path2 = uri_to_path(uri2).unwrap();
        assert_eq!(
            path2.to_str().unwrap().replace('\\', "/"),
            "W:/agilang-native-runtime/examples/native/hello.agi"
        );
        assert_eq!(path_to_uri(&path2), uri2);

        let uri3 = "file:///home/user/app/main.agi";
        let path3 = uri_to_path(uri3).unwrap();
        assert_eq!(
            path3.to_str().unwrap().replace('\\', "/"),
            "/home/user/app/main.agi"
        );
    }

    #[test]
    fn test_utf16_unicode_position_conversion() {
        let text = "fn main() -> i32:\n    let message = \"Hello 🌍\"\n    return unknown_value";
        let target = "unknown_value";
        let offset = text.find(target).unwrap();

        let (line, char_offset) = offset_to_position(text, offset);
        assert_eq!(line, 2);

        let reconstructed_offset = position_to_offset(text, line, char_offset);
        assert_eq!(reconstructed_offset, offset);
    }

    #[test]
    fn test_framed_message_transport() {
        let raw_message = "Content-Length: 13\r\n\r\nHello World!!";
        let mut cursor = Cursor::new(raw_message.as_bytes());
        let read = read_framed_message(&mut cursor).unwrap();
        assert_eq!(read, "Hello World!!");
    }
}
