#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyMessage {
    Text(String),
    Quest {
        room_name: String,
        quest_text: String,
        image_url: Option<String>,
    },
}
