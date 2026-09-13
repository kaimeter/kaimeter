// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Type shim for the vendored shadcn/ui primitives.
 *
 * `web/src/components/ui/` is stock shadcn code copied in wholesale, not written
 * for this project. In their JS form the components infer props with every
 * destructured key required (`className`, `variant`, `size`, …), so every call
 * site that omits one becomes a type error and buries the checks that matter.
 *
 * `jsconfig.json` resolves `@/components/ui/*` here instead, so `npm run
 * typecheck` sees our own code and the generated wire contract while ignoring
 * the primitives' internals. Vite is unaffected — it resolves the real files
 * through the alias in `vite.config.mjs`.
 */

type Component = (props: any) => any;

export const Accordion: Component;
export const AccordionContent: Component;
export const AccordionItem: Component;
export const AccordionTrigger: Component;
export const Alert: Component;
export const AlertDescription: Component;
export const AlertTitle: Component;
export const Badge: Component;
export const Button: Component;
export const Card: Component;
export const CardContent: Component;
export const CardDescription: Component;
export const CardHeader: Component;
export const CardTitle: Component;
export const Input: Component;
export const Label: Component;
export const Progress: Component;
export const Select: Component;
export const SelectContent: Component;
export const SelectItem: Component;
export const SelectTrigger: Component;
export const SelectValue: Component;
export const Separator: Component;
export const Table: Component;
export const TableBody: Component;
export const TableCell: Component;
export const TableHead: Component;
export const TableHeader: Component;
export const TableRow: Component;
export const Tabs: Component;
export const TabsContent: Component;
export const TabsList: Component;
export const TabsTrigger: Component;
export const Tooltip: Component;
export const TooltipContent: Component;
export const TooltipProvider: Component;
export const TooltipTrigger: Component;
