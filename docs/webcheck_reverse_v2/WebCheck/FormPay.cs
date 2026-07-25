using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormPay : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("ДодатиЗасібОплатиToolStripMenuItem")]
	private ToolStripMenuItem _ДодатиЗасібОплатиToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЗакритиToolStripMenuItem")]
	private ToolStripMenuItem _ЗакритиToolStripMenuItem;

	internal virtual DataGridView DG
	{
		[CompilerGenerated]
		get
		{
			return _DG;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			DataGridViewCellEventHandler value2 = DG_CellDoubleClick;
			DataGridViewCellEventHandler value3 = DG_CellContentClick;
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick -= value2;
				dG.CellContentClick -= value3;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick += value2;
				dG.CellContentClick += value3;
			}
		}
	}

	[field: AccessedThroughProperty("Column1")]
	internal virtual DataGridViewTextBoxColumn Column1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column2")]
	internal virtual DataGridViewTextBoxColumn Column2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column3")]
	internal virtual DataGridViewTextBoxColumn Column3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("MenuStrip1")]
	internal virtual MenuStrip MenuStrip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("МенюToolStripMenuItem")]
	internal virtual ToolStripMenuItem МенюToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ДодатиЗасібОплатиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДодатиЗасібОплатиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ДодатиЗасібОплатиToolStripMenuItem_Click;
			ToolStripMenuItem додатиЗасібОплатиToolStripMenuItem = _ДодатиЗасібОплатиToolStripMenuItem;
			if (додатиЗасібОплатиToolStripMenuItem != null)
			{
				додатиЗасібОплатиToolStripMenuItem.Click -= value2;
			}
			_ДодатиЗасібОплатиToolStripMenuItem = value;
			додатиЗасібОплатиToolStripMenuItem = _ДодатиЗасібОплатиToolStripMenuItem;
			if (додатиЗасібОплатиToolStripMenuItem != null)
			{
				додатиЗасібОплатиToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ЗакритиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЗакритиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ЗакритиToolStripMenuItem_Click;
			ToolStripMenuItem закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				закритиToolStripMenuItem.Click -= value2;
			}
			_ЗакритиToolStripMenuItem = value;
			закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				закритиToolStripMenuItem.Click += value2;
			}
		}
	}

	public FormPay()
	{
		base.Load += FormPay_Load;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormPay));
		this.DG = new System.Windows.Forms.DataGridView();
		this.Column1 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column2 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column3 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.MenuStrip1 = new System.Windows.Forms.MenuStrip();
		this.МенюToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ДодатиЗасібОплатиToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem1 = new System.Windows.Forms.ToolStripSeparator();
		this.ЗакритиToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		((System.ComponentModel.ISupportInitialize)this.DG).BeginInit();
		this.MenuStrip1.SuspendLayout();
		base.SuspendLayout();
		this.DG.AllowUserToAddRows = false;
		this.DG.AllowUserToDeleteRows = false;
		this.DG.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left | System.Windows.Forms.AnchorStyles.Right;
		this.DG.ColumnHeadersHeightSizeMode = System.Windows.Forms.DataGridViewColumnHeadersHeightSizeMode.AutoSize;
		this.DG.Columns.AddRange(this.Column1, this.Column2, this.Column3);
		this.DG.Location = new System.Drawing.Point(-1, 31);
		this.DG.Name = "DG";
		this.DG.ReadOnly = true;
		this.DG.RowHeadersWidth = 51;
		this.DG.RowTemplate.Height = 24;
		this.DG.Size = new System.Drawing.Size(1047, 453);
		this.DG.TabIndex = 1;
		this.Column1.HeaderText = "№";
		this.Column1.MinimumWidth = 6;
		this.Column1.Name = "Column1";
		this.Column1.ReadOnly = true;
		this.Column1.Width = 54;
		this.Column2.HeaderText = "Засіб оплати";
		this.Column2.MinimumWidth = 6;
		this.Column2.Name = "Column2";
		this.Column2.ReadOnly = true;
		this.Column2.Width = 450;
		this.Column3.HeaderText = "Форма оплати";
		this.Column3.MinimumWidth = 6;
		this.Column3.Name = "Column3";
		this.Column3.ReadOnly = true;
		this.Column3.Width = 171;
		this.MenuStrip1.ImageScalingSize = new System.Drawing.Size(20, 20);
		this.MenuStrip1.Items.AddRange(new System.Windows.Forms.ToolStripItem[1] { this.МенюToolStripMenuItem });
		this.MenuStrip1.Location = new System.Drawing.Point(0, 0);
		this.MenuStrip1.Name = "MenuStrip1";
		this.MenuStrip1.Size = new System.Drawing.Size(1045, 28);
		this.MenuStrip1.TabIndex = 2;
		this.MenuStrip1.Text = "MenuStrip1";
		this.МенюToolStripMenuItem.DropDownItems.AddRange(new System.Windows.Forms.ToolStripItem[3] { this.ДодатиЗасібОплатиToolStripMenuItem, this.ToolStripMenuItem1, this.ЗакритиToolStripMenuItem });
		this.МенюToolStripMenuItem.Name = "МенюToolStripMenuItem";
		this.МенюToolStripMenuItem.Size = new System.Drawing.Size(65, 24);
		this.МенюToolStripMenuItem.Text = "Меню";
		this.ДодатиЗасібОплатиToolStripMenuItem.Name = "ДодатиЗасібОплатиToolStripMenuItem";
		this.ДодатиЗасібОплатиToolStripMenuItem.Size = new System.Drawing.Size(243, 26);
		this.ДодатиЗасібОплатиToolStripMenuItem.Text = "Додати засіб оплати...";
		this.ToolStripMenuItem1.Name = "ToolStripMenuItem1";
		this.ToolStripMenuItem1.Size = new System.Drawing.Size(240, 6);
		this.ЗакритиToolStripMenuItem.Name = "ЗакритиToolStripMenuItem";
		this.ЗакритиToolStripMenuItem.Size = new System.Drawing.Size(243, 26);
		this.ЗакритиToolStripMenuItem.Text = "Закрити";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1045, 482);
		base.Controls.Add(this.DG);
		base.Controls.Add(this.MenuStrip1);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormPay";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Засоби і форми оплати";
		((System.ComponentModel.ISupportInitialize)this.DG).EndInit();
		this.MenuStrip1.ResumeLayout(false);
		this.MenuStrip1.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormPay_Load(object sender, EventArgs e)
	{
		LoadPayForms();
	}

	private void LoadPayForms()
	{
		checked
		{
			try
			{
				All.PayTax.StartLoad();
				DG.RowCount = 0;
				int payN = All.PayTax.PayN;
				for (int i = 1; i <= payN; i++)
				{
					DG.RowCount++;
					DG[0, DG.RowCount - 1].Value = i.ToString();
					DG[1, DG.RowCount - 1].Value = All.PayTax.get_PayName(i).ToUpper();
					DG[2, DG.RowCount - 1].Value = All.PayTax.get_PayISCASHname(i).ToUpper();
					if (i < 5)
					{
						DG.Rows[i - 1].Cells[0].Style.BackColor = Color.FromArgb(255, 225, 225);
					}
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("Форма Оплаты", "Ошибка заполнения таблицы с формами платежей", "Критическая ошибка");
				ProjectData.ClearProjectError();
			}
		}
	}

	private void DG_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		string eIdPay = DG.CurrentRow.Cells[0].Value.ToString();
		string eNamePay = DG.CurrentRow.Cells[1].Value.ToString();
		string eGrPay = DG.CurrentRow.Cells[2].Value.ToString();
		new FormAddPay(eIdPay, eNamePay, eGrPay).ShowDialog();
		LoadPayForms();
	}

	private void ЗакритиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void ДодатиЗасібОплатиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		new FormAddPay().ShowDialog();
		LoadPayForms();
	}

	private void DG_CellContentClick(object sender, DataGridViewCellEventArgs e)
	{
	}
}
