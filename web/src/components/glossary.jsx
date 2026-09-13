// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

/**
 * Glossary and primer content.
 *
 * Compliance terms are normative: the termbase in `locales/termbase.json` is the
 * locked rendering (审查 use the Chinese «隐含排放», not a paraphrase), and the
 * `tip*` keys carry the plain-words explanations. Both are surfaced through
 * shadcn primitives — a Tooltip for a term, an Accordion for the long-form
 * primer — so the styling stays the theme's job.
 */

import { Info } from 'lucide-react';

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { RichText } from '@/components/rich-text';
import { term } from '@/lib/i18n';
import { useLocale, useT } from '@/lib/use-i18n';

/**
 * An inline term with its definition on hover.
 *
 * The label is the locked termbase rendering where one exists, falling back to
 * the locale string, so a term is never silently paraphrased.
 */
export function GlossaryTerm({ termKey, labelKey, children = null }) {
  const t = useT();
  const locale = useLocale();
  const label = children ?? (labelKey ? t(labelKey) : term(locale, termKey));

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="decoration-muted-foreground/50 underline decoration-dotted underline-offset-4">
          {label}
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-sm">
        <RichText text={t(termKey)} />
      </TooltipContent>
    </Tooltip>
  );
}

/** A labelled figure with its explanation on hover — the `ⓘ` used on the cards. */
export function Hint({ labelKey, className = undefined }) {
  const t = useT();
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Info className={className ?? 'size-3.5 cursor-help opacity-60'} />
      </TooltipTrigger>
      <TooltipContent className="max-w-sm">
        <RichText text={t(labelKey)} />
      </TooltipContent>
    </Tooltip>
  );
}

/**
 * The first sentence of a primer paragraph, used as its accordion label. The
 * paragraphs open with a bolded claim, so the label is that claim with its
 * markup stripped.
 */
function Lead({ text }) {
  const plain = (text ?? '').replace(/<[^>]+>/g, '');
  const stop = plain.search(/[.:;]/);
  // flex-1 keeps the label left-aligned: the trigger is a justify-between row.
  return (
    <span className="flex-1 text-left">
      {stop > 12 ? plain.slice(0, stop) : plain.slice(0, 90)}
    </span>
  );
}

/**
 * The 60-second guide. This is the screen that answers "what is CBAM?" without
 * assuming any EU climate-law knowledge (primerTitle + primerP1..P4 + whatsNext).
 */
export function CbamPrimer({ variant = 'card' }) {
  const t = useT();
  const sections = ['primerP1', 'primerP2', 'primerP3', 'primerP4'];

  if (variant === 'alert') {
    return (
      <Alert>
        <Info />
        <AlertTitle>{t('primerTitle')}</AlertTitle>
        <AlertDescription className="space-y-3">
          {sections.map((key) => (
            <p key={key}>
              <RichText text={t(key)} />
            </p>
          ))}
        </AlertDescription>
      </Alert>
    );
  }

  return (
    <Card id="kaimeter-primer" className="scroll-mt-24">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Info className="size-4" />
          {t('primerTitle')}
        </CardTitle>
        <CardDescription>{t('etsPlainSummary')}</CardDescription>
      </CardHeader>
      <CardContent>
        <Accordion type="single" collapsible className="w-full">
          {sections.map((key, index) => (
            <AccordionItem key={key} value={key}>
              <AccordionTrigger className="text-left text-sm">
                <span className="text-muted-foreground mr-2 tabular-nums">{index + 1}.</span>
                <Lead text={t(key)} />
              </AccordionTrigger>
              <AccordionContent className="text-muted-foreground text-sm leading-relaxed">
                <RichText text={t(key)} />
              </AccordionContent>
            </AccordionItem>
          ))}
          <AccordionItem value="whatsNext">
            <AccordionTrigger className="text-left text-sm">{t('deadlinesTitle')}</AccordionTrigger>
            <AccordionContent className="text-muted-foreground text-sm leading-relaxed">
              <RichText text={t('whatsNext')} />
            </AccordionContent>
          </AccordionItem>
        </Accordion>
      </CardContent>
    </Card>
  );
}

/** The ETS price explainer: what the price is, and what a certificate is. */
export function EtsExplainer() {
  const t = useT();
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{t('etsTitle')}</CardTitle>
        <CardDescription>{t('etsPlainSummary')}</CardDescription>
      </CardHeader>
      <CardContent>
        <Accordion type="single" collapsible className="w-full">
          {['etsPlainP1', 'etsPlainP2', 'etsPlainP3', 'etsPlainP4'].map((key) => (
            <AccordionItem key={key} value={key}>
              <AccordionTrigger className="text-left text-sm">
                <Lead text={t(key)} />
              </AccordionTrigger>
              <AccordionContent className="text-muted-foreground text-sm leading-relaxed">
                <RichText text={t(key)} />
              </AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </CardContent>
    </Card>
  );
}
