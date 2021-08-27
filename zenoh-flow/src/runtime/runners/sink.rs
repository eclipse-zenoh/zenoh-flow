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

use crate::runtime::graph::link::ZFLinkReceiver;
use crate::runtime::message::ZFMessage;
use crate::types::{Token, ZFContext, ZFInput, ZFResult};
use crate::SinkTrait;
use futures::future;
use libloading::Library;
use std::collections::HashMap;

#[repr(C)]
pub struct ZFSinkDeclaration {
    pub rustc_version: &'static str,
    pub core_version: &'static str,
    pub register: unsafe extern "C" fn(
        Option<HashMap<String, String>>,
    ) -> ZFResult<Box<dyn SinkTrait + Send>>,
}

pub struct ZFSinkRunner {
    pub operator: Box<dyn SinkTrait + Send>,
    pub lib: Option<Library>,
    pub inputs: Vec<ZFLinkReceiver<ZFMessage>>,
}

impl ZFSinkRunner {
    pub fn new(operator: Box<dyn SinkTrait + Send>, lib: Option<Library>) -> Self {
        Self {
            operator,
            lib,
            inputs: vec![],
        }
    }

    pub fn add_input(&mut self, input: ZFLinkReceiver<ZFMessage>) {
        self.inputs.push(input);
    }

    pub async fn run(&mut self) -> ZFResult<()> {
        // WIP empty context
        let ctx = ZFContext::new(self.operator.get_state(), 0);

        loop {
            // we should start from an HashMap with all PortId and not ready tokens
            let mut msgs: HashMap<String, Token> = HashMap::new();

            for i in self.inputs.iter() {
                msgs.insert(i.id(), Token::NotReady);
            }

            let ir_fn = self.operator.get_input_rule(ctx.clone());

            let mut futs = vec![];
            for rx in self.inputs.iter() {
                futs.push(rx.recv()); // this should be peek(), but both requires mut
            }

            crate::run_input_rules!(ir_fn, msgs, futs, ctx);

            // Running
            let run_fn = self.operator.get_run(ctx.clone());
            let mut data = ZFInput::new();

            for (id, v) in msgs {
                log::debug!("[SINK] Sending data to run: {:?}", v);
                let (d, _) = v.split();
                if d.is_none() {
                    continue;
                }
                data.insert(id, d.unwrap());
            }

            run_fn(ctx.clone(), data).await?;

            //This depends on the Tokens...
            for rx in self.inputs.iter() {
                rx.discard().await?;
            }
        }
    }
}