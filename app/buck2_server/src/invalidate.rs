/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is dual-licensed under either the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree or the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree. You may select, at your option, one of the
 * above-listed licenses.
 */

use async_trait::async_trait;
use buck2_analysis::analysis::calculation::AnalysisKey;
use buck2_build_api::configure_targets::load_compatible_patterns;
use buck2_cli_proto::InvalidateCacheRequest;
use buck2_cli_proto::InvalidateCacheResponse;
use buck2_common::pattern::parse_from_cli::parse_patterns_from_cli_args;
use buck2_core::pattern::pattern_type::TargetPatternExtra;
use buck2_core::target::configured_target_label::ConfiguredTargetLabel;
use buck2_node::load_patterns::MissingTargetBehavior;
use buck2_node::nodes::configured::ConfiguredTargetNode;
use buck2_server_ctx::ctx::ServerCommandContextTrait;
use buck2_server_ctx::global_cfg_options::global_cfg_options_from_client_context;
use buck2_server_ctx::partial_result_dispatcher::NoPartialResult;
use buck2_server_ctx::partial_result_dispatcher::PartialResultDispatcher;
use buck2_server_ctx::template::ServerCommandTemplate;
use buck2_server_ctx::template::run_server_command;
use dice::DiceTransaction;
use dupe::Dupe;

use crate::ctx::ServerCommandContext;

pub(crate) async fn invalidate_command(
    ctx: &ServerCommandContext<'_>,
    partial_result_dispatcher: PartialResultDispatcher<NoPartialResult>,
    req: InvalidateCacheRequest,
) -> buck2_error::Result<InvalidateCacheResponse> {
    run_server_command(InvalidateServerCommand { req }, ctx, partial_result_dispatcher).await
}

struct InvalidateServerCommand {
    req: InvalidateCacheRequest,
}

#[async_trait]
impl ServerCommandTemplate for InvalidateServerCommand {
    type StartEvent = buck2_data::InvalidateCacheCommandStart;
    type EndEvent = buck2_data::InvalidateCacheCommandEnd;
    type Response = InvalidateCacheResponse;
    type PartialResult = NoPartialResult;

    async fn command(
        &self,
        server_ctx: &dyn ServerCommandContextTrait,
        _partial_result_dispatcher: PartialResultDispatcher<Self::PartialResult>,
        mut ctx: DiceTransaction,
    ) -> buck2_error::Result<Self::Response> {
        // Get global configuration options (use default if not specified)
        let target_cfg = buck2_cli_proto::TargetCfg::default();

        // Parse target patterns from CLI arguments
        let parsed_patterns = parse_patterns_from_cli_args::<TargetPatternExtra>(
            &mut ctx,
            &self.req.target_patterns,
            server_ctx.working_dir(),
        )
        .await?;

        // Get global configuration options
        let global_cfg_options =
            global_cfg_options_from_client_context(&target_cfg, server_ctx, &mut ctx).await?;

        // Load and configure targets
        let loaded_result = load_compatible_patterns(
            &mut ctx,
            parsed_patterns,
            &global_cfg_options,
            MissingTargetBehavior::Fail,
            false, // keep_going
        )
        .await?;

        // Collect the configured target labels to invalidate
        let targets_to_invalidate: Vec<ConfiguredTargetLabel> = loaded_result
            .compatible_targets
            .iter()
            .map(|node: &ConfiguredTargetNode| node.label().dupe())
            .collect();

        // Convert the transaction into an updater for invalidating keys
        let mut updater = ctx.into_updater();
        let mut dice_keys_invalidated: u64 = 0;
        for target in &targets_to_invalidate {
            // Invalidate the AnalysisKey for this target
            let analysis_key = AnalysisKey(target.dupe());
            updater.changed(vec![analysis_key])?;
            dice_keys_invalidated += 1;
        }

        // Commit the invalidations
        let _new_ctx = updater.commit().await;

        tracing::info!(
            "Invalidated {} DICE keys for {} targets",
            dice_keys_invalidated,
            targets_to_invalidate.len()
        );

        // TODO: Implement artifact invalidation if self.req.invalidate_artifacts is true
        // This would involve calling materializer.invalidate_many() for the target's outputs
        let artifacts_invalidated: u64 = 0;

        Ok(InvalidateCacheResponse {
            dice_keys_invalidated,
            artifacts_invalidated,
        })
    }

    fn end_event(&self, _response: &buck2_error::Result<Self::Response>) -> Self::EndEvent {
        buck2_data::InvalidateCacheCommandEnd {}
    }
}
