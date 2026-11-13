"use client";

import { useTheme } from "next-themes";
import { Toaster as Sonner } from "sonner";

/** Toast notifications that follow the light/dark theme. Use: toast.success("Saved") */
export function Toaster() {
  const { resolvedTheme } = useTheme();
  return (
    <Sonner
      theme={resolvedTheme === "dark" ? "dark" : "light"}
      richColors
      position="bottom-right"
    />
  );
}
