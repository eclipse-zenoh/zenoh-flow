//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

pub mod runners;

use super::instance::runners::connector::{ZenohReceiver, ZenohSender};
use super::instance::runners::operator::OperatorRunner;
use super::instance::runners::sink::SinkRunner;
use super::instance::runners::source::SourceRunner;
use super::instance::runners::RunnerKind;
use crate::model::connector::ZFConnectorKind;
use crate::model::link::LinkRecord;
use crate::runtime::dataflow::Dataflow;
use crate::runtime::InstanceContext;
use crate::types::{Input, Inputs, NodeId, Output, Outputs};
use crate::zferror;
use crate::zfresult::ErrorKind;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use uhlc::HLC;
use uuid::Uuid;

use self::runners::Runner;

/// The instance of a data flow graph.
/// It contains runtime information for the instance
/// and the [`InstanceContext`](`InstanceContext`)
pub struct DataflowInstance {
    pub(crate) instance_context: Arc<InstanceContext>,
    pub(crate) runners: HashMap<NodeId, Box<dyn Runner>>,
}

/// Creates the [`Link`](`Link`) between the `nodes` using `links`.
///
/// # Errors
/// An error variant is returned in case of:
/// -  port id is duplicated.
fn create_links(
    nodes: &[NodeId],
    links: &[LinkRecord],
    hlc: Arc<HLC>,
) -> Result<HashMap<NodeId, (Inputs, Outputs)>> {
    let mut io: HashMap<NodeId, (Inputs, Outputs)> = HashMap::with_capacity(nodes.len());

    for link_desc in links {
        let upstream_node = link_desc.from.node.clone();
        let downstream_node = link_desc.to.node.clone();

        // Nodes have been filtered based on their runtime. If the runtime of either one of the node
        // is not equal to that of the current runtime, the channels should not be created.
        if !nodes.contains(&upstream_node) || !nodes.contains(&downstream_node) {
            continue;
        }

        // FIXME Introduce a user-configurable maximum capacity on the links. This also requires
        // implementing a dropping policy.
        let (tx, rx) = flume::unbounded();
        let from = link_desc.from.output.clone();
        let to = link_desc.to.input.clone();

        match io.get_mut(&upstream_node) {
            Some((_, outputs)) => {
                outputs
                    .entry(from.clone())
                    .or_insert_with(|| Output::new(from, hlc.clone()))
                    .add(tx);
            }
            None => {
                let inputs = HashMap::new();

                let mut output = Output::new(from.clone(), hlc.clone());
                output.add(tx);

                let outputs = HashMap::from([(from, output)]);

                io.insert(upstream_node, (inputs, outputs));
            }
        }

        match io.get_mut(&downstream_node) {
            Some((inputs, _)) => {
                inputs
                    .entry(to.clone())
                    .or_insert_with(|| Input::new(to))
                    .add(rx);
            }
            None => {
                let outputs = HashMap::new();

                let mut input = Input::new(to.clone());
                input.add(rx);

                let inputs = HashMap::from([(to, input)]);

                io.insert(downstream_node, (inputs, outputs));
            }
        }
    }

    Ok(io)
}

impl DataflowInstance {
    /// Tries to instantiate the [`Dataflow`](`Dataflow`)
    ///
    /// This function is called by the runtime once the `Dataflow` object was
    /// created and validated.
    ///
    /// # Errors
    /// An error variant is returned in case of:
    /// - validation fails
    /// - connectors cannot be created
    pub fn try_instantiate(dataflow: Dataflow, hlc: Arc<HLC>) -> Result<Self> {
        // Gather all node ids to be able to generate (i) the links and (ii) the hash map containing
        // the runners.
        let mut node_ids: Vec<NodeId> = Vec::with_capacity(
            dataflow.sources.len()
                + dataflow.operators.len()
                + dataflow.sinks.len()
                + dataflow.connectors.len(),
        );

        node_ids.append(&mut dataflow.sources.keys().cloned().collect::<Vec<_>>());
        node_ids.append(&mut dataflow.operators.keys().cloned().collect::<Vec<_>>());
        node_ids.append(&mut dataflow.sinks.keys().cloned().collect::<Vec<_>>());
        node_ids.append(&mut dataflow.connectors.keys().cloned().collect::<Vec<_>>());

        // FIXME Filter?
        let mut links = create_links(&node_ids, &dataflow.links, hlc)?;

        let context = Arc::new(InstanceContext {
            flow_id: dataflow.flow_id,
            instance_id: dataflow.uuid,
            runtime: dataflow.context,
        });

        // The links were created, we can generate the Runners.
        let mut runners: HashMap<NodeId, Box<dyn Runner>> = HashMap::with_capacity(node_ids.len());

        for (id, source) in dataflow.sources.into_iter() {
            let (_, outputs) = links.remove(&id).ok_or_else(|| {
                zferror!(
                    ErrorKind::IOError,
                    "Links for Source < {} > were not created.",
                    &source.id
                )
            })?;
            runners.insert(
                id,
                Box::new(SourceRunner::new(Arc::clone(&context), source, outputs)),
            );
        }

        for (id, operator) in dataflow.operators.into_iter() {
            let (inputs, outputs) = links.remove(&operator.id).ok_or_else(|| {
                zferror!(
                    ErrorKind::IOError,
                    "Links for Operator < {} > were not created.",
                    &operator.id
                )
            })?;
            runners.insert(
                id,
                Box::new(OperatorRunner::new(
                    context.clone(),
                    operator,
                    inputs,
                    outputs,
                )),
            );
        }

        for (id, sink) in dataflow.sinks.into_iter() {
            let (inputs, _) = links.remove(&id).ok_or_else(|| {
                zferror!(
                    ErrorKind::IOError,
                    "Links for Sink < {} > were not created.",
                    &sink.id
                )
            })?;
            runners.insert(id, Box::new(SinkRunner::new(context.clone(), sink, inputs)));
        }

        for (id, connector) in dataflow.connectors.into_iter() {
            let (inputs, outputs) = links.remove(&id).ok_or_else(|| {
                zferror!(
                    ErrorKind::IOError,
                    "Links for Connector < {} > were not created.",
                    &connector.id
                )
            })?;
            match connector.kind {
                ZFConnectorKind::Sender => {
                    runners.insert(
                        id,
                        Box::new(ZenohSender::try_new(context.clone(), connector, inputs)?),
                    );
                }
                ZFConnectorKind::Receiver => {
                    runners.insert(
                        id,
                        Box::new(ZenohReceiver::try_new(context.clone(), connector, outputs)?),
                    );
                }
            }
        }

        Ok(Self {
            instance_context: context,
            runners,
        })
    }

    /// Returns the instance's `Uuid`.
    pub fn get_uuid(&self) -> Uuid {
        self.instance_context.instance_id
    }

    /// Returns the instance's `FlowId`.
    pub fn get_flow(&self) -> Arc<str> {
        self.instance_context.flow_id.clone()
    }

    /// Returns a copy of the `InstanceContext`.
    pub fn get_instance_context(&self) -> Arc<InstanceContext> {
        self.instance_context.clone()
    }

    /// Returns the `NodeId` for all the sources in this instance.
    pub fn get_sources(&self) -> Vec<NodeId> {
        self.runners
            .values()
            .filter(|runner| matches!(runner.get_kind(), RunnerKind::Source))
            .map(|runner| runner.get_id())
            .collect()
    }

    /// Returns the `NodeId` for all the sinks in this instance.
    pub fn get_sinks(&self) -> Vec<NodeId> {
        self.runners
            .values()
            .filter(|runner| matches!(runner.get_kind(), RunnerKind::Sink))
            .map(|runner| runner.get_id())
            .collect()
    }

    /// Returns the `NodeId` for all the sinks in this instance.
    pub fn get_operators(&self) -> Vec<NodeId> {
        self.runners
            .values()
            .filter(|runner| matches!(runner.get_kind(), RunnerKind::Operator))
            .map(|runner| runner.get_id())
            .collect()
    }

    /// Returns the `NodeId` for all the connectors in this instance.
    pub fn get_connectors(&self) -> Vec<NodeId> {
        self.runners
            .values()
            .filter(|runner| matches!(runner.get_kind(), RunnerKind::Connector))
            .map(|runner| runner.get_id())
            .collect()
    }

    /// Returns the `NodeId` for all the nodes in this instance.
    pub fn get_nodes(&self) -> Vec<NodeId> {
        self.runners
            .values()
            .map(|runner| runner.get_id())
            .collect()
    }

    /// Starts all the sources in this instance.
    ///
    ///
    /// **Note:** Not implemented.
    ///
    /// # Errors
    /// An error variant is returned in case of:
    /// -  sources already started.
    pub async fn start_sources(&mut self) -> Result<()> {
        Err(zferror!(ErrorKind::Unimplemented).into())
    }

    /// Starts all the nodes in this instance.
    ///
    /// **Note:** Not implemented.
    ///
    /// # Errors
    /// An error variant is returned in case of:
    /// -  nodes already started.
    pub async fn start_nodes(&mut self) -> Result<()> {
        Err(zferror!(ErrorKind::Unimplemented).into())
    }

    /// Stops all the sources in this instance.
    ///
    /// **Note:** Not implemented.
    ///
    /// # Errors
    /// An error variant is returned in case of:
    /// -  sources already stopped.
    pub async fn stop_sources(&mut self) -> Result<()> {
        Err(zferror!(ErrorKind::Unimplemented).into())
    }

    /// Stops all the sources in this instance.
    ///
    /// **Note:** Not implemented.
    ///
    /// # Errors
    /// An error variant is returned in case of:
    /// -  nodes already stopped.
    pub async fn stop_nodes(&mut self) -> Result<()> {
        Err(zferror!(ErrorKind::Unimplemented).into())
    }

    /// Checks if the given node is running.
    ///
    /// # Errors
    /// If fails if the node is not found.
    pub async fn is_node_running(&self, node_id: &NodeId) -> Result<bool> {
        if let Some(runner) = self.runners.get(node_id) {
            return Ok(runner.is_running().await);
        }

        Err(zferror!(ErrorKind::NodeNotFound(node_id.clone())).into())
    }

    /// Starts the given node.
    ///
    /// # Errors
    /// If fails if the node is not found.
    pub async fn start_node(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(runner) = self.runners.get_mut(node_id) {
            return runner.start().await;
        }

        Err(zferror!(ErrorKind::NodeNotFound(node_id.clone())).into())
    }

    /// Stops the given node.
    ///
    /// # Errors
    /// If fails if the node is not found or it is not running.
    pub async fn stop_node(&mut self, node_id: &NodeId) -> Result<()> {
        if let Some(runner) = self.runners.get_mut(node_id) {
            runner.stop().await?;
            return Ok(());
        }

        Err(zferror!(ErrorKind::NodeNotFound(node_id.clone())).into())
    }

    // /// Starts the recording for the given source.
    // ///
    // /// It returns the key expression where the recording is stored.
    // ///
    // /// # Errors
    // /// If fails if the node is not found.
    // pub async fn start_recording(&self, node_id: &NodeId) -> Result<String> {
    //     if let Some(runner) = self.runners.get(node_id) {
    //         return runner.start_recording().await;
    //     }

    //     Err(zferror!(ErrorKind::NodeNotFound(node_id.clone())).into())

    // }

    // /// Stops the recording for the given source.
    // ///
    // /// It returns the key expression where the recording is stored.
    // ///
    // /// # Errors
    // /// If fails if the node is not found.
    // pub async fn stop_recording(&self, node_id: &NodeId) -> Result<String> {
    //     if let Some(runner) = self.runners.get(node_id) {
    //         return runner.stop_recording().await;
    //     }

    //     Err(zferror!(ErrorKind::NodeNotFound(node_id.clone())).into())

    // }

    // /// Assumes the source is already stopped before calling the start replay!
    // /// This method is called by the runtime, that always check that the node
    // /// is not running prior to call this function.
    // /// If someone is using directly the DataflowInstance need to stop and check
    // /// if the node is running before calling this function.
    // ///
    // /// # Errors
    // /// It fails if:
    // /// - the source is not stopped
    // /// - the node is not a source
    // /// - the key expression is not point to a recording
    // pub async fn start_replay(&mut self, source_id: &NodeId, resource: String) -> Result<NodeId> {
    //     let runner = self
    //         .runners
    //         .get(source_id)
    //         .ok_or_else(|| ZFError::NodeNotFound(source_id.clone()))?;

    //     let mut outputs: Vec<(PortId, PortType)> = runner
    //         .get_outputs()
    //         .iter()
    //         .map(|(k, v)| (k.clone(), v.clone()))
    //         .collect();

    //     let (output_id, _output_type) = outputs
    //         .pop()
    //         .ok_or_else(|| ZFError::NodeNotFound(source_id.clone()))?;

    //     let replay_id: NodeId = format!(
    //         "replay-{}-{}-{}-{}",
    //         self.context.flow_id, self.context.instance_id, source_id, output_id
    //     )
    //     .into();

    //     let output_links = runner
    //         .get_outputs_links()
    //         .await
    //         .remove(&output_id)
    //         .ok_or_else(|| ZFError::PortNotFound((source_id.clone(), output_id.clone())))?;

    //     let replay_node = ZenohReplay::try_new(
    //         replay_id.clone(),
    //         self.context.clone(),
    //         source_id.clone(),
    //         output_id,
    //         _output_type,
    //         output_links,
    //         resource,
    //     )?;

    //     self.runners
    //         .insert(replay_id.clone(), Box::new(replay_node));
    //     Ok(replay_id)
    // }

    // /// Stops the recording for the given source.
    // ///
    // /// # Errors
    // /// If fails if the node is not found.
    // pub async fn stop_replay(&mut self, replay_id: &NodeId) -> Result<()> {
    //     self.stop_node(replay_id).await?;
    //     self.runners.remove(replay_id);
    //     Ok(())
    // }
}
