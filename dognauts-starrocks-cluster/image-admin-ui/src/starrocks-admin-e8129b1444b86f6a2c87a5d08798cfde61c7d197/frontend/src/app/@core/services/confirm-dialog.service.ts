import { Injectable } from '@angular/core';
import { NbDialogService } from '@nebular/theme';
import { Observable } from 'rxjs';
import { ConfirmDialogComponent } from '../components/confirm-dialog/confirm-dialog.component';

@Injectable({
  providedIn: 'root'
})
export class ConfirmDialogService {
  constructor(private dialogService: NbDialogService) {}

  confirm(
    title: string,
    message: string,
    confirmText: string = 'Confirm',
    cancelText: string = 'Cancel',
    confirmStatus: string = 'primary'
  ): Observable<boolean> {
    return this.dialogService.open(ConfirmDialogComponent, {
      context: {
        title,
        message,
        confirmText,
        cancelText,
        confirmStatus
      },
      hasBackdrop: true,
      closeOnBackdropClick: false,
      closeOnEsc: true,
      autoFocus: true
    }).onClose;
  }

  confirmDelete(itemName: string, additionalWarning?: string): Observable<boolean> {
    const message = additionalWarning 
      ? `Are you sure you want to delete "${itemName}"?\n\n${additionalWarning}`
      : `Are you sure you want to delete "${itemName}"?`;

    return this.confirm(
      'Confirm Deletion',
      message,
      'Delete',
      'Cancel',
      'danger'
    );
  }
}
