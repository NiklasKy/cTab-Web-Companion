import type { TacticalEntity } from "./protocol";

const LABEL_COLLATOR = new Intl.Collator("en", {
  numeric: true,
  sensitivity: "base"
});

export function sortTacticalEntitiesByLabel(
  entities: readonly TacticalEntity[]
): TacticalEntity[] {
  return [...entities].sort((left, right) =>
    LABEL_COLLATOR.compare(left.label, right.label)
      || left.id.localeCompare(right.id)
  );
}
