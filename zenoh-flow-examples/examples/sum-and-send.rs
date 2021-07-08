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
use std::collections::HashMap;
use zenoh_flow::runtime::message::ZFMessage;
use zenoh_flow::types::{
    DataTrait, FnInputRule, FnOutputRule, FnRun, InputRuleResult, OperatorTrait, OutputRuleResult,
    RunResult, StateTrait, Token, ZFContext, ZFError, ZFInput, ZFLinkId, ZFResult
};
use zenoh_flow::zenoh_flow_derive::ZFState;
use zenoh_flow::{downcast_mut, get_input, zf_data};
use zenoh_flow_examples::RandomData;

use async_std::sync::Arc;

#[derive(Serialize, Deserialize, Debug)]
struct SumAndSend {
    pub state: SumAndSendState,
}

#[derive(Serialize, Deserialize, Debug, Clone, ZFState)]
struct SumAndSendState {
    pub x: RandomData,
}

static INPUT: &str = "Number";
static OUTPUT: &str = "Number";

impl SumAndSend {
    pub fn new() -> Self {
        Self {
            state: SumAndSendState {
                x: RandomData { d: 0 },
            },
        }
    }

    pub fn ir_1(_ctx: ZFContext, inputs: &mut HashMap<ZFLinkId, Token>) -> InputRuleResult {
        if let Some(token) = inputs.get(INPUT) {
            match token {
                Token::Ready(_) => Ok(true),
                Token::NotReady(_) => Ok(false),
            }
        } else {
            Err(ZFError::MissingInput(String::from(INPUT)))
        }
    }

    pub fn run_1(ctx: ZFContext, mut inputs: ZFInput) -> RunResult {
        let mut results: HashMap<ZFLinkId, Arc<dyn DataTrait>> = HashMap::new();

        let mut guard = ctx.lock(); //getting the context
        let mut _state = downcast_mut!(SumAndSendState, guard.state).unwrap(); //getting and downcasting  state to right type

        let data = get_input!(RandomData, String::from(INPUT), inputs)?;

        let res = _state.x.d + data.d;
        let res = RandomData { d: res };
        _state.x = res.clone();

        results.insert(String::from(OUTPUT), zf_data!(res));
        Ok(results)
    }

    pub fn or_1(
        _ctx: ZFContext,
        outputs: HashMap<ZFLinkId, Arc<dyn DataTrait>>,
    ) -> OutputRuleResult {
        let mut results = HashMap::new();
        for (k, v) in outputs {
            // should be ZFMessage::from_data
            results.insert(k, Arc::new(ZFMessage::from_data(v)));
        }
        Ok(results)
    }
}

impl OperatorTrait for SumAndSend {
    fn get_input_rule(&self, ctx: ZFContext) -> Box<FnInputRule> {
        let gctx = ctx.lock();
        match gctx.mode {
            0 => Box::new(Self::ir_1),
            _ => panic!("No way"),
        }
    }

    fn get_output_rule(&self, ctx: ZFContext) -> Box<FnOutputRule> {
        let gctx = ctx.lock();
        match gctx.mode {
            0 => Box::new(Self::or_1),
            _ => panic!("No way"),
        }
    }

    fn get_run(&self, ctx: ZFContext) -> Box<FnRun> {
        let gctx = ctx.lock();
        match gctx.mode {
            0 => Box::new(Self::run_1),
            _ => panic!("No way"),
        }
    }

    fn get_state(&self) -> Box<dyn StateTrait> {
        Box::new(self.state.clone())
    }
}

// //Also generated by macro
zenoh_flow::export_operator!(register);

extern "C" fn register(
    configuration: Option<HashMap<String, String>>,
) -> ZFResult<Box<dyn zenoh_flow::OperatorTrait + Send>> {
    Ok(Box::new(SumAndSend::new()) as Box<dyn zenoh_flow::OperatorTrait + Send>)
}
