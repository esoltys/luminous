import { describe, it, expect } from "vitest";
import { importErrorMessage, profileNameFromPath } from "./eqImport";

describe("profileNameFromPath", () => {
  it("names the preset after the headphones in an AutoEq file name", () => {
    expect(profileNameFromPath("G:\\Downloads\\Anker Soundcore Life Q20 ParametricEq.txt")).toBe(
      "Anker Soundcore Life Q20"
    );
    expect(profileNameFromPath("/home/me/Sennheiser HD 600 ParametricEQ.txt")).toBe("Sennheiser HD 600");
  });

  it("keeps a file name that has no ParametricEQ suffix", () => {
    expect(profileNameFromPath("C:\\eq\\Studio monitors.txt")).toBe("Studio monitors");
  });
});

describe("importErrorMessage", () => {
  const err = (value: object) => JSON.stringify(value);

  it("lists every filter the equalizer can't reproduce, with a next step", () => {
    expect(
      importErrorMessage(
        err({
          code: "unsupported_filters",
          filters: [
            { line: 3, kind: "LP" },
            { line: 5, kind: "LSC 12dB" },
          ],
        })
      )
    ).toBe(
      "Unsupported filters: LP (line 3), LSC 12dB (line 5). Remove them, or export the profile from AutoEq, which only uses peak (PK), low-shelf (LSC) and high-shelf (HSC) filters."
    );
  });

  it("states the out-of-range value and the limits", () => {
    expect(
      importErrorMessage(err({ code: "out_of_range", line: 4, field: "gain", value: 23.456, min: -20, max: 20 }))
    ).toBe("Line 4: Gain 23.46 is outside the supported range (-20 to 20).");
    expect(importErrorMessage(err({ code: "preamp_out_of_range", value: -30, min: -24, max: 12 }))).toBe(
      "The preamp of -30 dB is outside the supported range (-24 to 12 dB)."
    );
  });

  it("counts filters past the limit", () => {
    expect(importErrorMessage(err({ code: "too_many_filters", count: 21, max: 20 }))).toBe(
      "This profile has 21 filters, but the equalizer supports up to 20."
    );
  });

  it("explains a taken preset name", () => {
    expect(importErrorMessage(err({ code: "duplicate_name" }))).toBe("A preset with this name already exists. Choose another name.");
  });

  it("falls back to a generic message for anything unrecognised", () => {
    expect(importErrorMessage("not json")).toBe("Couldn't import the profile.");
    expect(importErrorMessage(err({ code: "something_new" }))).toBe("Couldn't import the profile.");
  });
});
