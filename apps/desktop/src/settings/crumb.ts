/**
 * A page's sub-view in the Settings title: "Focus / Stats", t3code's
 * "Usage / All environments". The shell draws it; the page sets it from an
 * event handler, and the shell clears it on every page change.
 */

import { createContext } from "react";

export interface Crumb {
  label: string;
  /** Back to the page's own view. */
  back: () => void;
}

export const CrumbContext = createContext<(crumb: Crumb | null) => void>(() => {});
