import { describe, expect, it } from "vitest";
import { countryDisplayName, countryOptions, parseCountryInput } from "./country";

describe("country helpers", () => {
  it("names a code in the requested locale", () => {
    expect(countryDisplayName("ca", "en")).toBe("Canada");
    expect(countryDisplayName("DE", "fr")).toBe("Allemagne");
    expect(countryDisplayName("", "en")).toBe("");
    expect(countryDisplayName("ZZZ", "en")).toBe("");
  });

  it("lists regions sorted by localized name", () => {
    const options = countryOptions("en");
    expect(options.length).toBeGreaterThan(200);
    expect(options.find((o) => o.code === "FI")?.name).toBe("Finland");
    const names = options.map((o) => o.name);
    expect(names).toEqual([...names].sort((a, b) => a.localeCompare(b, "en")));
  });

  it("parses codes and names, telling blank from unrecognised", () => {
    expect(parseCountryInput(" ca ")).toBe("CA");
    expect(parseCountryInput("canada")).toBe("CA");
    expect(parseCountryInput("Allemagne", "fr")).toBe("DE");
    expect(parseCountryInput("Germany", "fr")).toBe("DE");
    expect(parseCountryInput("")).toBe("");
    expect(parseCountryInput("Narnia")).toBeNull();
    expect(parseCountryInput("ZZ")).toBeNull();
  });
});
