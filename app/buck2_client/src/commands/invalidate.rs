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
use buck2_cli_proto::InvalidateCacheRequest;
use buck2_cli_proto::InvalidateCacheResponse;
use buck2_client_ctx::client_ctx::ClientCommandContext;
use buck2_client_ctx::common::BuckArgMatches;
use buck2_client_ctx::common::CommonBuildConfigurationOptions;
use buck2_client_ctx::common::CommonCommandOptions;
use buck2_client_ctx::common::CommonEventLogOptions;
use buck2_client_ctx::common::CommonStarlarkOptions;
use buck2_client_ctx::common::ui::CommonConsoleOptions;
use buck2_client_ctx::daemon::client::BuckdClientConnector;
use buck2_client_ctx::daemon::client::NoPartialResultHandler;
use buck2_client_ctx::events_ctx::EventsCtx;
use buck2_client_ctx::exit_result::ExitResult;
use buck2_client_ctx::streaming::StreamingCommand;

/// Invalidate DICE cache entries for specific targets without restarting the daemon.
///
/// This allows selectively busting caches for specific targets while preserving
/// caches for others, useful when you want to rebuild specific targets without
/// the overhead of a full daemon restart.
#[derive(Debug, clap::Parser)]
pub struct InvalidateCommand {
    /// Target patterns to invalidate (e.g., //my:target)
    #[clap(required = true, value_name = "TARGET")]
    target_patterns: Vec<String>,

    /// Also delete materialized artifacts from disk for the invalidated targets
    #[clap(long)]
    invalidate_artifacts: bool,

    #[clap(flatten)]
    common_opts: CommonCommandOptions,
}

#[async_trait(?Send)]
impl StreamingCommand for InvalidateCommand {
    const COMMAND_NAME: &'static str = "invalidate";

    async fn exec_impl(
        self,
        buckd: &mut BuckdClientConnector,
        matches: BuckArgMatches<'_>,
        ctx: &mut ClientCommandContext<'_>,
        events_ctx: &mut EventsCtx,
    ) -> ExitResult {
        let context = ctx.client_context(matches, &self)?;
        let response: InvalidateCacheResponse = buckd
            .with_flushing()
            .invalidate(
                InvalidateCacheRequest {
                    context: Some(context),
                    target_patterns: self.target_patterns.clone(),
                    invalidate_artifacts: self.invalidate_artifacts,
                },
                events_ctx,
                ctx.console_interaction_stream(&self.common_opts.console_opts),
                &mut NoPartialResultHandler,
            )
            .await??;

        buck2_client_ctx::eprintln!(
            "Invalidated {} DICE keys",
            response.dice_keys_invalidated
        )?;
        if response.artifacts_invalidated > 0 {
            buck2_client_ctx::eprintln!(
                "Invalidated {} artifacts",
                response.artifacts_invalidated
            )?;
        }

        ExitResult::success()
    }

    fn console_opts(&self) -> &CommonConsoleOptions {
        &self.common_opts.console_opts
    }

    fn event_log_opts(&self) -> &CommonEventLogOptions {
        &self.common_opts.event_log_opts
    }

    fn build_config_opts(&self) -> &CommonBuildConfigurationOptions {
        &self.common_opts.config_opts
    }

    fn starlark_opts(&self) -> &CommonStarlarkOptions {
        &self.common_opts.starlark_opts
    }
}
