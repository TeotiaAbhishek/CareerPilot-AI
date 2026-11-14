import { MobileNav } from "./mobile-nav";
import { ThemeToggle } from "./theme-toggle";

export function Topbar() {
  return (
    <header className="sticky top-0 z-20 flex h-14 items-center gap-2 border-b bg-background/80 px-4 backdrop-blur sm:px-6 print:hidden">
      <MobileNav />
      <div className="flex-1" />
      <ThemeToggle />
    </header>
  );
}
