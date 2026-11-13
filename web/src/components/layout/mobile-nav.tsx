"use client";

import { Menu, X } from "lucide-react";
import { Suspense, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";

import { Brand } from "./brand";
import { NavLinks, NavLinksFallback } from "./nav-links";

/** Hamburger button + slide-in drawer, only below the lg breakpoint. */
export function MobileNav() {
  const [open, setOpen] = useState(false);

  // Close with Escape, and stop the page behind from scrolling while open.
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && setOpen(false);
    document.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKey);
      document.body.style.overflow = "";
    };
  }, [open]);

  return (
    <div className="lg:hidden">
      <Button variant="ghost" size="icon" aria-label="Open menu" onClick={() => setOpen(true)}>
        <Menu />
      </Button>

      {open ? (
        <div className="fixed inset-0 z-50" role="dialog" aria-modal="true" aria-label="Menu">
          <div className="absolute inset-0 bg-black/50" onClick={() => setOpen(false)} />
          <div className="absolute inset-y-0 left-0 flex w-72 flex-col border-r bg-sidebar px-3 py-5 shadow-xl">
            <div className="flex items-center justify-between">
              <Brand />
              <Button variant="ghost" size="icon" aria-label="Close menu" onClick={() => setOpen(false)}>
                <X />
              </Button>
            </div>
            <div className="mt-8">
              <Suspense fallback={<NavLinksFallback />}>
                <NavLinks onNavigate={() => setOpen(false)} />
              </Suspense>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
