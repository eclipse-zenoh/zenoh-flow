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

use async_std::sync::Arc;
use std::collections::HashMap;
use zenoh_flow::zenoh_flow_derive::ZFState;
use zenoh_flow::{
    default_input_rule, default_output_rule, downcast_mut, get_input, zf_data, ZFComponent,
    ZFComponentInputRule, ZFComponentOutput, ZFComponentOutputRule, ZFDataTrait, ZFOperatorTrait,
    ZFResult,
};
use zenoh_flow_examples::ZFUsize;

#[derive(Debug)]
struct SumAndSend;

#[derive(Debug, Clone, ZFState)]
struct SumAndSendState {
    pub x: ZFUsize,
}

static INPUT: &str = "Number";
static OUTPUT: &str = "Sum";

impl ZFOperatorTrait for SumAndSend {
    fn run(
        &self,
        dyn_state: &mut Box<dyn zenoh_flow::ZFStateTrait>,
        inputs: &mut HashMap<String, zenoh_flow::runtime::message::ZFDataMessage>,
    ) -> zenoh_flow::ZFResult<HashMap<zenoh_flow::ZFPortID, Arc<dyn ZFDataTrait>>> {
        let mut results: HashMap<String, Arc<dyn ZFDataTrait>> = HashMap::new();

        // Downcasting state to right type
        let mut state = downcast_mut!(SumAndSendState, dyn_state).unwrap();

        let (_, data) = get_input!(ZFUsize, String::from(INPUT), inputs)?;

        let res = ZFUsize(state.x.0 + data.0);
        state.x = res.clone();

        results.insert(String::from(OUTPUT), zf_data!(res));
        Ok(results)
    }
}

impl ZFComponentInputRule for SumAndSend {
    fn input_rule(
        &self,
        state: &mut Box<dyn zenoh_flow::ZFStateTrait>,
        tokens: &mut HashMap<String, zenoh_flow::Token>,
    ) -> zenoh_flow::ZFResult<bool> {
        default_input_rule(state, tokens)
    }
}

impl ZFComponentOutputRule for SumAndSend {
    fn output_rule(
        &self,
        state: &mut Box<dyn zenoh_flow::ZFStateTrait>,
        outputs: &HashMap<String, Arc<dyn ZFDataTrait>>,
    ) -> zenoh_flow::ZFResult<HashMap<zenoh_flow::ZFPortID, ZFComponentOutput>> {
        default_output_rule(state, outputs)
    }
}

impl ZFComponent for SumAndSend {
    fn initial_state(
        &self,
        _configuration: &Option<HashMap<String, String>>,
    ) -> Box<dyn zenoh_flow::ZFStateTrait> {
        Box::new(SumAndSendState { x: ZFUsize(0) })
    }
}

// Also generated by macro
zenoh_flow::export_operator!(register);

fn register() -> ZFResult<Box<dyn ZFOperatorTrait + Send>> {
    Ok(Box::new(SumAndSend) as Box<dyn ZFOperatorTrait + Send>)
}
