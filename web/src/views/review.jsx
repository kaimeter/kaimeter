// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { useState } from 'react';
import { ClipboardCheck, FileSearch, ShieldAlert, ShieldCheck } from 'lucide-react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { api } from '@/lib/api';
import { useT } from '@/lib/use-i18n';

/**
 * The verifier's view (R28/R33).
 *
 * Verification is the verifier's act, never a self-declaration — which is why
 * this is a separate persona rather than a button on the declarant's screen.
 */
export function Review() {
  const t = useT();
  const [decision, setDecision] = useState(
    /** @type {'accept' | 'finding' | null} */ (null),
  );

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <FileSearch className="size-4" />
            {t('reviewTab')}
          </CardTitle>
          <CardDescription>{t('reviewSub')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-muted-foreground text-sm">{t('verifyStatus')}</span>
            <Badge variant="outline">{t('unverified')}</Badge>
            <Separator orientation="vertical" className="!h-4" />
            <span className="text-muted-foreground text-sm">{t('verifierCheck')}</span>
          </div>

          <div className="flex flex-wrap gap-3">
            <Button
              onClick={() => setDecision('accept')}
              variant={decision === 'accept' ? 'default' : 'outline'}
            >
              <ShieldCheck />
              {t('reviewVerifyBtn')}
            </Button>
            <Button
              onClick={() => setDecision('finding')}
              variant={decision === 'finding' ? 'destructive' : 'outline'}
            >
              <ShieldAlert />
              {t('verifyPackFileBtn')}
            </Button>
          </div>

          <p className="text-muted-foreground text-xs leading-relaxed">{t('reviewNote')}</p>
          <Separator />
          <p className="text-muted-foreground text-xs leading-relaxed">{t('verifyLaterHint')}</p>
        </CardContent>
      </Card>

      {decision === 'accept' ? (
        <Alert>
          <ClipboardCheck />
          <AlertTitle>{t('toastVerifiedPack')}</AlertTitle>
          <AlertDescription>{t('onceVerified')}</AlertDescription>
        </Alert>
      ) : null}

      {decision === 'finding' ? (
        <Alert variant="destructive">
          <ShieldAlert />
          <AlertTitle>{t('unverified')}</AlertTitle>
          <AlertDescription>{t('verifyLaterHint')}</AlertDescription>
        </Alert>
      ) : null}

      {!api.served ? (
        <p className="text-muted-foreground text-xs">{t('verifyNeedsCore')}</p>
      ) : null}
    </div>
  );
}
