use tungstenite::protocol::WebSocketConfig;

pub fn get_config() -> WebSocketConfig {
  WebSocketConfig::default()
    .read_buffer_size(mb_to_bytes(1.5))
    .write_buffer_size(mb_to_bytes(1.5))
    .max_message_size(None)
    .max_frame_size(None)
}

pub fn mb_to_bytes(mb: f32) -> usize {
  (mb * 1024.0 * 1024.0) as usize
}
