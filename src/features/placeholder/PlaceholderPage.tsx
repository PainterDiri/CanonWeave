import type { ReactNode } from "react";
import { EmptyState, SectionTitle } from "@/components/ui";

export function PlaceholderPage({
  eyebrow,
  title,
  icon,
  emptyTitle,
  description,
  action,
}: {
  eyebrow: string;
  title: string;
  icon: ReactNode;
  emptyTitle: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="fade-in mx-auto max-w-[1120px] px-8 py-7">
      <SectionTitle eyebrow={eyebrow} title={title} />
      <EmptyState icon={icon} title={emptyTitle} description={description} action={action} />
    </div>
  );
}
