import {
  BarChart3,
  Briefcase,
  FileText,
  FlaskConical,
  KanbanSquare,
  LayoutDashboard,
  Settings,
  UserRound,
  type LucideIcon,
} from "lucide-react";

export type NavItem = {
  href: string;
  label: string;
  icon: LucideIcon;
};

/** The sidebar menu. Add a page here and it appears everywhere (desktop + mobile). */
export const NAV_ITEMS: NavItem[] = [
  { href: "/", label: "Dashboard", icon: LayoutDashboard },
  { href: "/jobs", label: "Jobs", icon: Briefcase },
  { href: "/evals", label: "Evals", icon: FlaskConical },
  { href: "/applications", label: "Applications", icon: KanbanSquare },
  { href: "/insights", label: "Insights", icon: BarChart3 },
  { href: "/documents", label: "Documents", icon: FileText },
  { href: "/profile", label: "Profile", icon: UserRound },
  { href: "/settings", label: "Settings", icon: Settings },
];

/** "/" only matches itself; other items also match their sub-pages (/jobs/123). */
export function isActive(pathname: string, href: string) {
  return href === "/" ? pathname === "/" : pathname === href || pathname.startsWith(`${href}/`);
}
