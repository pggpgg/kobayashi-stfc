/** Compile-time guard: optimizer transport types have a single generated contract. */
import type { OptimizeResponse, OptimizeStatusResponse } from "./api";
import type { components } from "./api/generated";

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;
type Assert<T extends true> = T;

export type OptimizerResponseMatchesSchema = Assert<
  Equal<OptimizeResponse, components["schemas"]["OptimizeResponse"]>
>;
export type OptimizerStatusMatchesSchema = Assert<
  Equal<OptimizeStatusResponse, components["schemas"]["OptimizeStatusResponse"]>
>;
