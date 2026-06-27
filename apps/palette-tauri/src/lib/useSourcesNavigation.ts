import { useCallback, useState } from "react";

import { ACTIONS } from "@/lib/actions";
import type { PaletteAction } from "@/lib/actions";
import type { SourceSortMode } from "@/lib/sourcesModel";

/** Owns Sources/Domains navigation filters and row-level action routing. */
export function useSourcesNavigation(
  requestSubmit: (action: PaletteAction, argumentOverride?: string) => void,
) {
  const [sourcesDrillFilter, setSourcesDrillFilter] = useState("");
  const [sourcesFilter, setSourcesFilter] = useState("");
  const [sourcesSort, setSourcesSort] = useState<SourceSortMode>("chunks");
  const [sourcesGrouped, setSourcesGrouped] = useState(false);

  const clearSourcesFilter = useCallback(() => {
    setSourcesDrillFilter("");
    setSourcesFilter("");
  }, []);

  const clearSourcesForAction = useCallback(
    (action: PaletteAction) => {
      if (action.subcommand === "sources") clearSourcesFilter();
    },
    [clearSourcesFilter],
  );

  const onRunAction = useCallback(
    (subcommand: string, argument: string) => {
      if (subcommand === "sources") clearSourcesFilter();
      const action = ACTIONS.find((candidate) => candidate.subcommand === subcommand);
      if (action) requestSubmit(action, argument);
    },
    [clearSourcesFilter, requestSubmit],
  );

  const onDrillDomain = useCallback(
    (domain: string) => {
      setSourcesDrillFilter(domain);
      setSourcesFilter(domain);
      const action = ACTIONS.find((candidate) => candidate.subcommand === "sources");
      if (action) requestSubmit(action, "");
    },
    [requestSubmit],
  );

  return {
    sourcesDrillFilter,
    sourcesFilter,
    sourcesSort,
    sourcesGrouped,
    setSourcesFilter,
    setSourcesSort,
    setSourcesGrouped,
    clearSourcesFilter,
    clearSourcesForAction,
    onRunAction,
    onDrillDomain,
  };
}
