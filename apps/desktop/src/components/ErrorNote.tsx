import { useErrorText } from "@/lib/text";
import { cn } from "@/lib/utils";

/** A failed command, shown in place of what it would have produced. */
export function ErrorNote({
  error,
  className,
}: {
  error: unknown;
  className?: string;
}) {
  const { summary, detail } = useErrorText()(error);
  return (
    <div role="alert" className={cn("text-sm", className)}>
      <p className="text-destructive">{summary}</p>
      <p className="mt-1 text-xs text-muted-foreground">{detail}</p>
    </div>
  );
}
