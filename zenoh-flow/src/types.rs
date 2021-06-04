//
// Copyright (c) 2017, 2021 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

use serde::{Deserialize, Serialize};

use crate::message::{Message, ZFCtrlMessage, ZFMessage};
use crate::operator::StateTrait;
use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;

// Placeholder types
pub type ZFOperatorId = String;
pub type ZFTimestamp = u128;
pub type ZFLinkId = u128;

#[derive(Debug)]
pub enum ZFError {
    GenericError,
    SerializationError,
    DeseralizationError,
    RecvError(flume::RecvError),
    SendError(String),
    MissingInput(ZFLinkId),
    InvalidData(ZFLinkId),
}

impl From<flume::RecvError> for ZFError {
    fn from(err: flume::RecvError) -> Self {
        Self::RecvError(err)
    }
}

impl<T> From<flume::SendError<T>> for ZFError {
    fn from(err: flume::SendError<T>) -> Self {
        Self::SendError(format!("{:?}", err))
    }
}

pub struct ZFContext {
    // pub state: Arc<Mutex<dyn StateTrait>>,
    pub state: Box<dyn StateTrait>,
    pub mode: u128,
}

pub type ZFResult<T> = Result<T, ZFError>;

// Maybe TokenActions should be always sent back to the OperatorRunner,
// to allow it the management of the data in the links.
pub enum OperatorResult {
    InResult(Result<(bool, HashMap<ZFLinkId, TokenAction>), ZFError>),
    RunResult(ZFError), // This may be just ZFError
    OutResult(Result<HashMap<ZFLinkId, ZFMessage>, ZFError>),
}

pub type OperatorRun = dyn Fn(&mut ZFContext, &HashMap<ZFLinkId, Option<Arc<ZFMessage>>>) -> OperatorResult
    + Send
    + Sync
    + 'static;

pub type ZFSourceResult = Result<Vec<Message>, ZFError>;
pub type ZFSourceRun = dyn Fn(&mut ZFContext) -> ZFSourceResult + Send + Sync + 'static; // This should be a future, Sources can do I/O

pub type ZFSinkResult = Result<(), ZFError>;
pub type ZFSinkRun =
    dyn Fn(&mut ZFContext, Vec<&ZFMessage>) -> ZFSinkResult + Send + Sync + 'static; // This should be a future, Sinks can do I/O

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ZFOperatorKind {
    Source,
    Sink,
    Compute,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZFOperatorDescription {
    pub id: ZFOperatorId,
    pub name: String,
    pub inputs: u8,
    pub outputs: u8,
    pub lib: String,
    pub kind: ZFOperatorKind,
}

impl std::fmt::Display for ZFOperatorDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ZFOperatorConnection {
    pub from: ZFOperatorId,
    pub to: ZFOperatorId,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DataFlowDescription {
    pub operators: Vec<ZFOperatorDescription>,
    pub connections: Vec<ZFOperatorConnection>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TokenAction {
    Consume, // Default, data is passed to the run and consumed from the "thing"
    Drop,    // Data is dropped
    KeepRun, // Data is passed to the run and kept in the "thing"
    Keep,    // Data is kept in the "thing"
    Wait,    //Waits the Data, this is applicable only to NotReadyToken
}

#[derive(Debug, Clone, Default)]
pub struct NotReadyToken {
    pub ts: ZFTimestamp,
}

impl NotReadyToken {
    /// Creates a `NotReadyToken` with its timestamp set to 0.
    pub fn new() -> Self {
        NotReadyToken { ts: 0 }
    }
}

#[derive(Debug, Clone)]
pub struct ReadyToken<T: ?Sized> {
    pub ts: ZFTimestamp,
    pub data: Arc<T>,
    pub action: TokenAction,
}

#[derive(Debug, Clone)]
pub enum Token<T: ?Sized> {
    NotReady(NotReadyToken),
    Ready(ReadyToken<T>),
}

impl<T> Token<T> {
    pub fn new_ready(ts: ZFTimestamp, data: Arc<T>) -> Self {
        Self::Ready(ReadyToken::<T> {
            ts,
            data,
            action: TokenAction::Consume,
        })
    }

    pub fn new_not_ready(ts: ZFTimestamp) -> Self {
        Self::NotReady(NotReadyToken { ts })
    }

    pub fn is_ready(&self) -> bool {
        match self {
            Self::Ready(_) => true,
            _ => false,
        }
    }

    pub fn is_not_ready(&self) -> bool {
        match self {
            Self::NotReady(_) => true,
            _ => false,
        }
    }

    pub fn consume(&mut self) -> ZFResult<()> {
        match self {
            Self::Ready(ref mut ready) => {
                ready.action = TokenAction::Consume;
                Ok(())
            }
            _ => Err(ZFError::GenericError),
        }
    }

    pub fn drop(&mut self) -> ZFResult<()> {
        match self {
            Self::Ready(ref mut ready) => {
                ready.action = TokenAction::Drop;
                Ok(())
            }
            _ => Err(ZFError::GenericError),
        }
    }

    pub fn keep_run(&mut self) -> ZFResult<()> {
        match self {
            Self::Ready(ref mut ready) => {
                ready.action = TokenAction::KeepRun;
                Ok(())
            }
            _ => Err(ZFError::GenericError),
        }
    }

    pub fn keep(&mut self) -> ZFResult<()> {
        match self {
            Self::Ready(ref mut ready) => {
                ready.action = TokenAction::Keep;
                Ok(())
            }
            _ => Err(ZFError::GenericError),
        }
    }

    pub fn data<'a>(&'a self) -> ZFResult<&'a T> {
        match self {
            Self::Ready(ready) => Ok(&ready.data),
            _ => Err(ZFError::GenericError),
        }
    }

    pub fn action<'a>(&'a self) -> &'a TokenAction {
        match self {
            Self::Ready(ready) => &ready.action,
            Self::NotReady(_) => &TokenAction::Wait,
        }
    }

    pub fn split(self) -> (Option<Arc<T>>, TokenAction) {
        match self {
            Self::Ready(ready) => (Some(ready.data), ready.action),
            Self::NotReady(_) => (None, TokenAction::Wait),
        }
    }
}
