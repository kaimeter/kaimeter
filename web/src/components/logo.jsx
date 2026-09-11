// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { cn } from '@/lib/utils';

/**
 * The Kaimeter mark: a rising growth curve over the metering rings, inside a
 * circular clip. Kept in one place so the topbar and any larger placement stay
 * identical.
 *
 * This is the detailed artwork, which needs roughly 32px to read — the dashed
 * rings and the highlight on the bud disappear below that. For favicon-sized
 * use, `MarkSimple` drops those details rather than letting them turn to mush.
 */
export function Mark({ className, ...props }) {
  return (
    <svg
      viewBox="0 0 100 100"
      role="img"
      aria-label="Kaimeter"
      className={cn('size-8', className)}
      {...props}
    >
      <defs>
        <clipPath id="km-mark-clip">
          <circle cx="50" cy="50" r="44" />
        </clipPath>
      </defs>
      <g clipPath="url(#km-mark-clip)">
        <circle cx="50" cy="50" r="36" fill="none" stroke="#052E16" strokeWidth="3" strokeDasharray="6 6" opacity=".5" />
        <circle cx="50" cy="50" r="28" fill="none" stroke="#22C55E" strokeWidth="3" strokeDasharray="6 6" opacity=".5" />
        <circle cx="50" cy="50" r="20" fill="none" stroke="#052E16" strokeWidth="3" strokeDasharray="6 6" opacity=".5" />
        <path
          d="M45 55v30m10-35v35"
          fill="none"
          stroke="#052E16"
          strokeWidth="6"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          d="m45 85-7 5m7-5-3 7M55 85l-7 5m7-5-3 7"
          fill="none"
          stroke="#052E16"
          strokeWidth="5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          d="M30 70q0-30 20-40t30-5"
          fill="none"
          stroke="#22C55E"
          strokeWidth="8"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <circle cx="80" cy="25" r="10" fill="#052E16" />
        <circle cx="84" cy="22" r="3" fill="#BBF7D0" />
        <path d="m90 20 8 4-8 4z" fill="#22C55E" />
        <path
          d="M20 92h60"
          fill="none"
          stroke="#22C55E"
          strokeWidth="4"
          strokeLinecap="round"
          opacity=".6"
        />
      </g>
    </svg>
  );
}

/**
 * Simplified mark for small sizes: the growth curve and the bud only. The rings
 * and highlight are dropped because below ~32px they merge into an unreadable
 * grey disc.
 */
export function MarkSimple({ className, ...props }) {
  return (
    <svg
      viewBox="0 0 100 100"
      role="img"
      aria-label="Kaimeter"
      className={cn('size-8', className)}
      {...props}
    >
      <circle cx="50" cy="50" r="48" fill="#052E16" />
      <path
        d="M26 76q0-34 22-45t32-6"
        fill="none"
        stroke="#22C55E"
        strokeWidth="11"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="76" cy="27" r="11" fill="#22C55E" />
    </svg>
  );
}
