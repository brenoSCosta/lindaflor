interface SectionLabelProps {
  children: string;
}

export function SectionLabel({ children }: SectionLabelProps) {
  return (
    <div className="flex items-center gap-2">
      <div className="h-4 w-1 rounded-full bg-primary" />
      <span className="text-sm font-bold tracking-wide text-primary">
        {children}
      </span>
    </div>
  );
}
