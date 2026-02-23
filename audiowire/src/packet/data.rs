use audiowire_derive::{Deserialize, Serialize};
use audiowire_serde::{Deserialize, Serialize};

use super::{
    message::{EncodedMessage, IntoMessage},
    stream::StreamId,
};

#[derive(Serialize, Deserialize)]
pub struct ClientData<T: Serialize + Deserialize>(pub StreamId, pub T);

#[derive(Serialize, Deserialize)]
pub struct ServerData<T: Serialize + Deserialize>(pub T);

macro_rules! data_messages {
    (
        $($type:ty),+
    ) => {
        $(
            impl<T: Serialize + Deserialize> IntoMessage for $type {
                fn into_message(self) -> EncodedMessage<Self> {
                    EncodedMessage::from(self)
                }
            }

            impl<T: Serialize + Deserialize> From<$type> for EncodedMessage<$type> {
                fn from(value: $type) -> Self {
                    Self {
                        code: 10,
                        message: value,
                    }
                }
            }
        )+
    };
}

data_messages!(ClientData<T>, ServerData<T>);
