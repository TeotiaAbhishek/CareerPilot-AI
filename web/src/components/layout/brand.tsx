import { Compass } from "lucide-react";
import Link from "next/link";

export function Brand() {
  return (
    <Link href="/" className="flex items-center gap-2.5 px-3 font-semibold tracking-tight">
      <span className="flex size-8 items-center justify-center rounded-lg bg-gradient-to-br from-primary to-fuchsia-500 text-white shadow-sm">
        <Compass className="size-4.5" aria-hidden />
      </span>
      CareerPilot
    </Link>
  );
}
