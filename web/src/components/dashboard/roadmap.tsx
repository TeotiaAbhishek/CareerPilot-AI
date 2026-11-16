import { CheckCircle2, Circle } from "lucide-react";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

const FEATURES = [
  { name: "Foundation and design system", done: true },
  { name: "Jobs: SEEK scraper, list and detail", done: true },
  { name: "Relevance scoring with Ollama", done: true },
  { name: "Profile, facts and password vault", done: true },
  { name: "Tailored documents on Apply", done: true },
  { name: "Browser apply: form filling and Quick Apply", done: true },
  { name: "Browser apply: logins and email codes", done: true },
  { name: "One-click Apply and tracker", done: true },
  { name: "Email drafts and follow-ups", done: false },
  { name: "Suggestions assistant", done: false },
];

/** Build progress. Tick a feature here when it ships. */
export function Roadmap() {
  const done = FEATURES.filter((f) => f.done).length;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Build progress</CardTitle>
        <CardDescription>
          {done} of {FEATURES.length} features shipped
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="mb-4 h-1.5 overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-primary transition-all"
            style={{ width: `${(done / FEATURES.length) * 100}%` }}
          />
        </div>
        <ul className="flex flex-col gap-2.5">
          {FEATURES.map((feature) => (
            <li key={feature.name} className="flex items-center gap-2.5 text-sm">
              {feature.done ? (
                <CheckCircle2 className="size-4 text-success" aria-hidden />
              ) : (
                <Circle className="size-4 text-muted-foreground/50" aria-hidden />
              )}
              <span className={cn(!feature.done && "text-muted-foreground")}>{feature.name}</span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
