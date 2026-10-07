import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// jsdom does not lay anything out, so it has no scrolling to do.
Element.prototype.scrollIntoView = vi.fn();

afterEach(() => {
  cleanup();
});
