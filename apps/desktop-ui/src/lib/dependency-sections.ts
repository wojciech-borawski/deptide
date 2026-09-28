import { allSections, type DependencySection } from "@/api/types";

export type SectionField = "dependencies" | "peerDependencies" | "devDependencies";

export interface SectionGroup<T> {
  section: DependencySection;
  field: SectionField;
  items: T[];
}

export const sectionField: Record<DependencySection, SectionField> = {
  dependencies: "dependencies",
  peer: "peerDependencies",
  dev: "devDependencies",
};

function inPrecedenceOrder(sections: readonly DependencySection[]): DependencySection[] {
  return allSections.filter((section) => sections.includes(section));
}

export function primarySection(sections: readonly DependencySection[]): DependencySection {
  return inPrecedenceOrder(sections)[0] ?? "dependencies";
}

export function otherSections(sections: readonly DependencySection[]): DependencySection[] {
  const primary = primarySection(sections);
  return inPrecedenceOrder(sections).filter((section) => section !== primary);
}

export function groupChoicesBySection<T extends { sections: readonly DependencySection[] }>(
  choices: readonly T[],
): SectionGroup<T>[] {
  return allSections
    .map((section) => ({
      section,
      field: sectionField[section],
      items: choices.filter((choice) => primarySection(choice.sections) === section),
    }))
    .filter((group) => group.items.length > 0);
}
