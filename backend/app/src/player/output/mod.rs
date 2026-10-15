mod playout;

pub(crate) use playout::{play_plain, play_plain_from};

use crate::{player::controller::ChannelManager, utils::errors::ServiceError};

/// Player entry point backed by backend/engine.
pub async fn player(manager: ChannelManager) -> Result<(), ServiceError> {
    playout::player(manager).await
}
