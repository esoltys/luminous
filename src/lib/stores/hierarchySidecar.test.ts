import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { hierarchySidecarStore } from "./hierarchySidecar.svelte";
import { toastStore } from "./toast.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const LIBRARY = "D:\\Music";

function mockStatus(status: { path: string | null; error: string | null }) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) =>
    cmd === "get_default_library" ? status : null
  );
}

async function initCapturingListener() {
  let emit: ((e: { payload: { path: string; message: string } }) => void) | undefined;
  vi.mocked(listen).mockImplementationOnce(async (_event, cb) => {
    emit = cb as typeof emit;
    return () => {};
  });
  await hierarchySidecarStore.init();
  return (message: string) => emit!({ payload: { path: LIBRARY, message } });
}

describe("hierarchySidecarStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const m of [...toastStore.messages]) toastStore.dismiss(m.id);
  });

  it("toasts a load error reported at startup once, even when the event also delivers it", async () => {
    mockStatus({ path: LIBRARY, error: "expected value at line 1" });
    const emit = await initCapturingListener();
    emit("expected value at line 1");

    expect(hierarchySidecarStore.path).toBe(LIBRARY);
    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0]).toMatchObject({
      variant: "error",
      text: `Couldn't load the genre hierarchy from ${LIBRARY}: expected value at line 1`,
    });
  });

  it("shows the error on the store and toasts a new failure after a clean load", async () => {
    mockStatus({ path: LIBRARY, error: null });
    const emit = await initCapturingListener();
    expect(toastStore.messages).toHaveLength(0);

    emit("unsupported version 2");
    expect(hierarchySidecarStore.error).toBe("unsupported version 2");
    expect(toastStore.messages).toHaveLength(1);
  });

  it("picks up a default library the backend linked on its own", async () => {
    let changed: (() => void) | undefined;
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      if (event === "default-library-changed") changed = cb as () => void;
      return () => {};
    });
    mockStatus({ path: null, error: null });
    await hierarchySidecarStore.init();
    expect(hierarchySidecarStore.path).toBeNull();

    mockStatus({ path: LIBRARY, error: null });
    changed!();
    await vi.waitFor(() => expect(hierarchySidecarStore.path).toBe(LIBRARY));
  });

  it("refreshes the status after a refused link and passes the backend's reason on", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "set_default_library") throw `${LIBRARY} is not available`;
      if (cmd === "get_default_library") return { path: null, error: null };
      return null;
    });

    await expect(hierarchySidecarStore.set(LIBRARY)).rejects.toBe(`${LIBRARY} is not available`);
    expect(invoke).toHaveBeenCalledWith("set_default_library", { path: LIBRARY });
    expect(hierarchySidecarStore.path).toBeNull();
  });
});
