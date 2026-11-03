import { Component, OnInit } from '@angular/core';
import { FormBuilder, FormGroup, Validators, AbstractControl, ValidationErrors } from '@angular/forms';
import { NbDialogRef } from '@nebular/theme';
import { SystemFunction } from '../../../../@core/data/system-function';

@Component({
  selector: 'ngx-edit-function-dialog',
  template: `
    <nb-card>
      <nb-card-header>
        <div class="d-flex align-items-center">
          <nb-icon icon="edit-outline" class="mr-2"></nb-icon>
          <h5 class="mb-0">Edit Custom Feature</h5>
        </div>
      </nb-card-header>
      <nb-card-body>
        <form [formGroup]="editFunctionForm" (ngSubmit)="onSubmit()">
          <div class="row">
            <div class="col-md-6">
              <div class="form-group">
                <label for="category_name" class="label">Category Name *</label>
                <input
                  type="text"
                  id="category_name"
                  formControlName="category_name"
                  nbInput
                  fullWidth
                  placeholder="Enter a category name"
                  status="basic"
                  readonly
                />
                <small class="text-hint">
                  The category name cannot be changed when editing a feature.
                </small>
              </div>
            </div>
            <div class="col-md-6">
              <div class="form-group">
                <label for="function_name" class="label">Feature Name *</label>
                <input
                  type="text"
                  id="function_name"
                  formControlName="function_name"
                  nbInput
                  fullWidth
                  placeholder="Enter a feature name"
                  status="basic"
                />
                <div *ngIf="editFunctionForm.get('function_name')?.invalid && editFunctionForm.get('function_name')?.touched" class="text-danger small mt-1">
                  Feature name is required.
                </div>
              </div>
            </div>
          </div>

          <div class="form-group">
            <label for="description" class="label">Feature Description *</label>
            <textarea
              id="description"
              formControlName="description"
              nbInput
              fullWidth
              rows="3"
              placeholder="Enter a feature description"
              status="basic"
            ></textarea>
            <div *ngIf="editFunctionForm.get('description')?.invalid && editFunctionForm.get('description')?.touched" class="text-danger small mt-1">
              Feature description is required.
            </div>
          </div>

          <div class="form-group">
            <label for="sql_query" class="label">SQL Query *</label>
            <textarea
              id="sql_query"
              formControlName="sql_query"
              nbInput
              fullWidth
              rows="6"
              placeholder="Enter a SQL query (SELECT and SHOW statements only)"
              status="basic"
            ></textarea>
            <div *ngIf="editFunctionForm.get('sql_query')?.invalid && editFunctionForm.get('sql_query')?.touched" class="text-danger small mt-1">
              SQL query is required.
            </div>
            <small class="text-hint">
              Only SELECT and SHOW SQL statements are supported.
            </small>
          </div>
        </form>
      </nb-card-body>
      <nb-card-footer>
        <div class="d-flex justify-content-end gap-2">
          <button
            type="button"
            nbButton
            status="basic"
            (click)="onCancel()"
          >
            <nb-icon icon="close-outline"></nb-icon>
            Cancel
          </button>
          <button
            type="button"
            nbButton
            status="primary"
            [disabled]="editFunctionForm.invalid"
            (click)="onSubmit()"
          >
            <nb-icon icon="checkmark-outline"></nb-icon>
            Save
          </button>
        </div>
      </nb-card-footer>
    </nb-card>
  `,
  styles: [`
    :host {
      display: block;
      width: 100%;
    }
    
    .form-group {
      margin-bottom: 1.5rem;
    }
    
    .label {
      font-weight: 600;
      margin-bottom: 0.5rem;
      display: block;
      color: var(--text-basic-color);
    }
    
    .text-danger {
      color: var(--color-danger-default) !important;
    }
    
    .text-hint {
      color: var(--text-hint-color) !important;
    }
    
    .gap-2 {
      gap: 0.5rem;
    }
    
    .gap-2 > * + * {
      margin-left: 0.5rem;
    }
    
    input[readonly] {
      background-color: var(--background-basic-color-2) !important;
      color: var(--text-hint-color) !important;
      cursor: not-allowed !important;
    }
  `]
})
export class EditFunctionDialogComponent implements OnInit {
  editFunctionForm: FormGroup;
  function: SystemFunction;

  constructor(
    private fb: FormBuilder,
    private dialogRef: NbDialogRef<EditFunctionDialogComponent>
  ) {
    this.editFunctionForm = this.fb.group({
      category_name: ['', [Validators.required, Validators.maxLength(100), this.trimValidator]],
      function_name: ['', [Validators.required, Validators.maxLength(100), this.trimValidator]],
      description: ['', [Validators.required, Validators.maxLength(500), this.trimValidator]],
      sql_query: ['', [Validators.required, this.trimValidator]]
    });
  }

  // 自定义验证器：检查trim后是否为空
  private trimValidator(control: AbstractControl): ValidationErrors | null {
    if (control.value && typeof control.value === 'string') {
      const trimmed = control.value.trim();
      if (trimmed.length === 0) {
        return { required: true };
      }
    }
    return null;
  }

  ngOnInit() {
    // 预填充表单数据
    if (this.function) {
      this.editFunctionForm.patchValue({
        category_name: this.function.categoryName || '',
        function_name: this.function.functionName || '',
        description: this.function.description || '',
        sql_query: this.function.sqlQuery || ''
      });
    }
  }

  onSubmit() {
    if (this.editFunctionForm.valid) {
      const formValue = this.editFunctionForm.value;
      const updatedFunction: SystemFunction = {
        ...this.function,
        categoryName: formValue.category_name?.trim() || '',
        functionName: formValue.function_name?.trim() || '',
        description: formValue.description?.trim() || '',
        sqlQuery: formValue.sql_query?.trim() || ''
      };
      this.dialogRef.close(updatedFunction);
    }
  }

  onCancel() {
    this.dialogRef.close();
  }
}
