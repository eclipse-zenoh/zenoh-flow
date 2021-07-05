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
use zenoh_flow::{
    downcast, downcast_mut, get_input,
    operator::{
        DataTrait, FnInputRule, FnSinkRun, FutSinkResult, InputRuleResult, SinkTrait, StateTrait,
    },
    serde::{Deserialize, Serialize},
    types::{Token, ZFContext, ZFError, ZFInput, ZFLinkId, ZFResult},
    zenoh_flow_macros::ZFState,
    zf_empty_state, zf_spin_lock,
};
use zenoh_flow_examples::{ZFBytes, ZFOpenCVBytes};

use opencv::{highgui, prelude::*};

static INPUT: &str = "Frame";

#[derive(Debug)]
struct VideoSink {}

#[derive(Serialize, Deserialize, ZFState, Clone, Debug)]
struct VideoState {
    pub window_name: String,
}

impl VideoSink {
    pub fn new() -> Self {
        let window_name = &format!("Video-Sink");
        highgui::named_window(window_name, 1).unwrap();
        Self {}
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

    pub async fn run_1(_ctx: ZFContext, mut inputs: ZFInput) -> ZFResult<()> {
        let mut results: HashMap<ZFLinkId, Arc<dyn DataTrait>> = HashMap::new();

        let window_name = &format!("Video-Sink");

        let data = get_input!(ZFBytes, String::from(INPUT), inputs).unwrap();

        let decoded = match opencv::imgcodecs::imdecode(
            &opencv::types::VectorOfu8::from_iter(data.bytes.clone()),
            opencv::imgcodecs::IMREAD_COLOR,
        ) {
            Ok(d) => d,
            Err(e) => panic!("Unable to decode {:?}", e),
        };

        if decoded.size().unwrap().width > 0 {
            match highgui::imshow(window_name, &decoded) {
                Ok(_) => (),
                Err(e) => eprintln!("Error when display {:?}", e),
            };
        }

        highgui::wait_key(10);

        Ok(())
    }
}

impl SinkTrait for VideoSink {
    fn get_input_rule(&self, _ctx: ZFContext) -> Box<FnInputRule> {
        Box::new(Self::ir_1)
    }

    fn get_run(&self, ctx: ZFContext) -> FnSinkRun {
        Box::new(|ctx: ZFContext, inputs: ZFInput| -> FutSinkResult {
            Box::pin(Self::run_1(ctx, inputs))
        })
    }

    fn get_state(&self) -> Box<dyn StateTrait> {
        zf_empty_state!()
    }
}

// //Also generated by macro
zenoh_flow::export_sink!(register);

extern "C" fn register(
    _configuration: Option<HashMap<String, String>>,
) -> ZFResult<Box<dyn zenoh_flow::operator::SinkTrait + Send>> {
    Ok(Box::new(VideoSink::new()) as Box<dyn zenoh_flow::operator::SinkTrait + Send>)
}
