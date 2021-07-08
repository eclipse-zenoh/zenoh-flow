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
use rand::Rng;
use std::collections::HashMap;
use zenoh_flow::{
    operator::{
        DataTrait, FnOutputRule, FnSourceRun, FutRunResult, RunResult, SourceTrait, StateTrait,
    },
    serde::{Deserialize, Serialize},
    types::{ZFContext, ZFInput, ZFLinkId, ZFResult},
    zenoh_flow_derive::ZFState,
    zf_data, zf_empty_state,
};
use zenoh_flow_examples::RandomData;

static SOURCE: &str = "Number";

#[derive(Serialize, Deserialize, Debug, ZFState)]
struct ExampleRandomSource {}

impl ExampleRandomSource {
    async fn run_1(_ctx: ZFContext) -> RunResult {
        let mut results: HashMap<ZFLinkId, Arc<dyn DataTrait>> = HashMap::new();
        let d = RandomData {
            d: rand::random::<u64>(),
        };
        results.insert(String::from(SOURCE), zf_data!(d));
        async_std::task::sleep(std::time::Duration::from_secs(1)).await;
        Ok(results)
    }
}

impl SourceTrait for ExampleRandomSource {
    fn get_run(&self, ctx: ZFContext) -> FnSourceRun {
        Box::new(|ctx: ZFContext| -> FutRunResult { Box::pin(Self::run_1(ctx)) })
    }

    fn get_output_rule(&self, _ctx: ZFContext) -> Box<FnOutputRule> {
        Box::new(zenoh_flow::operator::default_output_rule)
    }

    fn get_state(&self) -> Box<dyn StateTrait> {
        zf_empty_state!()
    }
}

// //Also generated by macro
zenoh_flow::export_source!(register);

extern "C" fn register(
    configuration: Option<HashMap<String, String>>,
) -> ZFResult<Box<dyn zenoh_flow::operator::SourceTrait + Send>> {
    Ok(Box::new(ExampleRandomSource {}) as Box<dyn zenoh_flow::operator::SourceTrait + Send>)
}
