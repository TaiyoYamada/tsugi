import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { api, isCommandError } from "@/lib/api";
import { useSettings } from "@/lib/settings-store";

type Opener = {
  /** Asks for a folder and opens it. */
  choose: () => Promise<void>;
  openPath: (path: string) => Promise<void>;
  /** Asks whether to set tsugi up, when it is needed. Render it once. */
  dialog: ReactNode;
};

/**
 * Opening a repository: the folder picker, and the question whether to set
 * tsugi up in a repository that does not have it yet. `onOpen` receives the
 * root of the repository, `onError` what went wrong.
 */
export function useRepoOpener(
  onOpen: (root: string) => void,
  onError: (error: unknown) => void,
): Opener {
  const { t } = useTranslation();
  const { watch } = useSettings();
  // A repository that tsugi is not set up in yet, waiting for the answer.
  const [toSetUp, setToSetUp] = useState<string | null>(null);

  async function openPath(path: string) {
    try {
      onOpen(await api.openRepo(path, watch));
    } catch (failure) {
      if (isCommandError(failure) && failure.code === "not_initialized") {
        setToSetUp(path);
      } else {
        onError(failure);
      }
    }
  }

  async function choose() {
    const path = await openDialog({ directory: true, multiple: false });
    if (path) await openPath(path);
  }

  async function setUp(path: string) {
    setToSetUp(null);
    try {
      onOpen(await api.initRepo(path, watch));
    } catch (failure) {
      onError(failure);
    }
  }

  const dialog = (
    <Dialog
      open={toSetUp !== null}
      onOpenChange={(open) => !open && setToSetUp(null)}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("picker.setUpTitle")}</DialogTitle>
          <DialogDescription>{t("picker.setUpDescription")}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => setToSetUp(null)}>
            {t("common.cancel")}
          </Button>
          <Button onClick={() => toSetUp && setUp(toSetUp)}>
            {t("picker.setUp")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );

  return { choose, openPath, dialog };
}
