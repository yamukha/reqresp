// Simple Req-Resp Protocol

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Req(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resp(pub Vec<u8>);
