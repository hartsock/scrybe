#!/usr/bin/env node
'use strict';

// Umbrella launcher. `scrybe-ai` depends on `@scrybe-ai/cli`; delegate straight
// to its bin shim, which resolves and execs the platform binary (and exits).
require('@scrybe-ai/cli/bin/scrybe.cjs');
