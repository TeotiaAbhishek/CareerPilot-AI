import { Suspense } from "react";

import { Brand } from "./brand";
import { NavLinks, NavLinksFallback } from "./nav-links";

/** Fixed sidebar on desktop (lg and up). On mobile the same links live in MobileNav. */
export function Sidebar() {
  return (
    <aside className="fixed inset-y-0 left-0 z-30 hidden w-64 flex-col border-r bg-sidebar px-3 py-5 lg:flex">
      <Brand />
      <div className="mt-8 flex-1">
        <Suspense fallback={<NavLinksFallback />}>
          <NavLinks />
        </Suspense>
      </div>
      <p className="px-3 text-xs text-muted-foreground">Local-first · your data stays on this machine</p>
    </aside>
  );
}
