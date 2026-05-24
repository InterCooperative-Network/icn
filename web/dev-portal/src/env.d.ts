/// <reference path="../.astro/types.d.ts" />

import type { Identity } from "./lib/scope-policy";

declare global {
  namespace App {
    interface Locals {
      identity: Identity;
    }
  }
}

export {};
