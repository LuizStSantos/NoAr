mod playout;

pub(crate) use playout::{
    channel_logo, load_logo, load_lshape, play_plain, play_plain_from, preload_later, refresh_logo,
};

use crate::{player::controller::ChannelManager, utils::errors::ServiceError};

/// Player entry point backed by backend/engine.
pub async fn player(manager: ChannelManager) -> Result<(), ServiceError> {
    playout::player(manager).await
}
