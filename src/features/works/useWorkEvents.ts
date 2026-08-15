import { useEffect } from "react";

import { normalizeAppError } from "../../domain/work";
import { useWorkStoreContext } from "./WorkStoreProvider";

export const useWorkEvents = () => {
  const { client, store } = useWorkStoreContext();

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void client
      .listenToWorkEvents((event) => {
        if (disposed) {
          return;
        }
        store.getState().applyEvent(event);
      })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
          return client.drainAssignmentEventOutbox().then(() => {
            if (!disposed) {
              store.getState().setSubscriptionError(null);
            }
          });
        }
      })
      .catch((error: unknown) => {
        if (!disposed) {
          store
            .getState()
            .setSubscriptionError(normalizeAppError(error));
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [client, store]);
};
