// Turns window.__LUMINOUS_SCRIPT__ into an object scenes can call from Node:
// `api.navigate.to("home")` becomes one evaluate() of that call in the page.
import type { DevtoolsDriver } from "../devtools-driver";
import type { RemoteApi } from "./types";

type Evaluator = Pick<DevtoolsDriver, "evaluate">;

export function createRemoteApi(driver: Evaluator): RemoteApi {
  const at = (path: string[]): unknown =>
    new Proxy(() => {}, {
      get: (_target, key) => (typeof key === "string" && key !== "then" ? at([...path, key]) : undefined),
      apply: (_target, _this, args) =>
        driver.evaluate(
          async (p: string[], a: unknown[]) => {
            const root = window.__LUMINOUS_SCRIPT__ as unknown as Record<string, unknown> | undefined;
            if (!root) throw new Error("window.__LUMINOUS_SCRIPT__ is missing: the app is not a dev-mode build.");
            let owner: any = root;
            for (const key of p.slice(0, -1)) owner = owner?.[key];
            const method = owner?.[p[p.length - 1]];
            if (typeof method !== "function") throw new Error(`Scripting API has no method "${p.join(".")}".`);
            return await method.apply(owner, a);
          },
          path,
          args
        ),
    });
  return at([]) as RemoteApi;
}
