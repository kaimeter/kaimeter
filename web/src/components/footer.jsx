// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Site footer.
 *
 * Everything here links to something that exists. Two entries a footer normally
 * carries are deliberately absent:
 *
 * - **Cookies.** Kaimeter sets no cookies and makes no network requests when
 *   opened from `file://` (R22). A cookie policy would describe a practice the
 *   product does not have.
 * - **Terms of Service.** That is a legal document, and one has not been
 *   written. A placeholder link would be worse than no link.
 *
 * Both are one line away in `FOOTER_LEGAL` if the project publishes a hosted
 * version or a landing page.
 */

import { ExternalLink, FileText, GitBranch, Scale } from 'lucide-react';

import { Separator } from '@/components/ui/separator';
import { useT } from '@/lib/use-i18n';

/** Package metadata, mirrored from Cargo.toml. */
export const PROJECT = {
  license: 'Apache-2.0',
  maintainer: 'Keldrion, LLC',
  repository: 'https://github.com/kaimeter/kaimeter',
  year: 2026,
};

/**
 * Legal pages that would appear here once they exist. Left empty on purpose —
 * see the module comment. Add `{ key, href }` entries to render them.
 */
export const FOOTER_LEGAL = [];

function FooterLink({ href, children, external = false }) {
  return (
    <a
      href={href}
      className="text-muted-foreground hover:text-foreground text-sm transition-colors"
      {...(external ? { target: '_blank', rel: 'noreferrer noopener' } : {})}
    >
      {children}
    </a>
  );
}

export function Footer({ onShowPrimer }) {
  const t = useT();
  const { license, maintainer, repository, year } = PROJECT;

  return (
    <footer className="mt-16 border-t">
      <div className="mx-auto max-w-7xl px-6 py-10">
        <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
          <div className="space-y-3">
            <h2 className="text-sm font-semibold">{t('brand')}</h2>
            <p className="text-muted-foreground text-sm leading-relaxed">{t('footerBlurb')}</p>
          </div>

          <div className="space-y-3">
            <h2 className="text-sm font-semibold">{t('footerProduct')}</h2>
            <ul className="space-y-2">
              <li>
                <button
                  type="button"
                  onClick={onShowPrimer}
                  className="text-muted-foreground hover:text-foreground text-sm transition-colors"
                >
                  {t('primerTitle')}
                </button>
              </li>
              <li>
                <FooterLink href="#top">{t('footerBackToTop')}</FooterLink>
              </li>
            </ul>
          </div>

          <div className="space-y-3">
            <h2 className="text-sm font-semibold">{t('footerSource')}</h2>
            <ul className="space-y-2">
              <li>
                <FooterLink href={repository} external>
                  <span className="inline-flex items-center gap-1.5">
                    <GitBranch className="size-3.5" />
                    {t('footerRepository')}
                    <ExternalLink className="size-3 opacity-60" />
                  </span>
                </FooterLink>
              </li>
              <li>
                <FooterLink href={`${repository}/blob/main/LICENSE`} external>
                  <span className="inline-flex items-center gap-1.5">
                    <Scale className="size-3.5" />
                    {t('footerLicense', { license })}
                  </span>
                </FooterLink>
              </li>
              <li>
                <FooterLink href={`${repository}/blob/main/NOTICE`} external>
                  <span className="inline-flex items-center gap-1.5">
                    <FileText className="size-3.5" />
                    {t('footerNotice')}
                  </span>
                </FooterLink>
              </li>
              {FOOTER_LEGAL.map(({ key, href }) => (
                <li key={key}>
                  <FooterLink href={href}>{t(key)}</FooterLink>
                </li>
              ))}
            </ul>
          </div>

          <div className="space-y-3">
            <h2 className="text-sm font-semibold">{t('footerMaintainedBy')}</h2>
            <p className="text-muted-foreground text-sm leading-relaxed">
              {t('footerMaintainerBody', { maintainer })}
            </p>
          </div>
        </div>

        <Separator className="my-8" />

        <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
          <p className="text-muted-foreground text-xs">
            © {year} {maintainer}. {t('footerRights')}
          </p>
          <p className="text-muted-foreground text-xs">{t('footerOfflineNote')}</p>
        </div>
      </div>
    </footer>
  );
}
