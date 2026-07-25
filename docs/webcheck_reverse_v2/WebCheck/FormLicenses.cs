using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
public class FormLicenses : Form
{
	private IContainer components;

	private string TINs;

	private AccountantОffice AO;

	[field: AccessedThroughProperty("DG")]
	internal virtual DataGridView DG
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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

	[field: AccessedThroughProperty("Column4")]
	internal virtual DataGridViewTextBoxColumn Column4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column5")]
	internal virtual DataGridViewTextBoxColumn Column5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column6")]
	internal virtual DataGridViewTextBoxColumn Column6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
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
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormLicenses));
		this.DG = new System.Windows.Forms.DataGridView();
		this.Column1 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column2 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column3 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column4 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column5 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		this.Column6 = new System.Windows.Forms.DataGridViewTextBoxColumn();
		((System.ComponentModel.ISupportInitialize)this.DG).BeginInit();
		base.SuspendLayout();
		this.DG.AllowUserToAddRows = false;
		this.DG.AllowUserToDeleteRows = false;
		this.DG.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left | System.Windows.Forms.AnchorStyles.Right;
		this.DG.ColumnHeadersHeightSizeMode = System.Windows.Forms.DataGridViewColumnHeadersHeightSizeMode.AutoSize;
		this.DG.Columns.AddRange(this.Column1, this.Column2, this.Column3, this.Column4, this.Column5, this.Column6);
		this.DG.Location = new System.Drawing.Point(2, 12);
		this.DG.Name = "DG";
		this.DG.ReadOnly = true;
		this.DG.RowHeadersWidth = 51;
		this.DG.RowTemplate.Height = 24;
		this.DG.Size = new System.Drawing.Size(1156, 541);
		this.DG.TabIndex = 1;
		this.Column1.HeaderText = "TIN";
		this.Column1.MinimumWidth = 6;
		this.Column1.Name = "Column1";
		this.Column1.ReadOnly = true;
		this.Column1.Width = 90;
		this.Column2.HeaderText = "Назва";
		this.Column2.MinimumWidth = 6;
		this.Column2.Name = "Column2";
		this.Column2.ReadOnly = true;
		this.Column2.Width = 136;
		this.Column3.HeaderText = "FN";
		this.Column3.MinimumWidth = 6;
		this.Column3.Name = "Column3";
		this.Column3.ReadOnly = true;
		this.Column3.Width = 81;
		this.Column4.HeaderText = "Адреса";
		this.Column4.MinimumWidth = 6;
		this.Column4.Name = "Column4";
		this.Column4.ReadOnly = true;
		this.Column4.Width = 333;
		this.Column5.HeaderText = "Ліцензія";
		this.Column5.MinimumWidth = 6;
		this.Column5.Name = "Column5";
		this.Column5.ReadOnly = true;
		this.Column5.Width = 90;
		this.Column6.HeaderText = "Днів";
		this.Column6.MinimumWidth = 6;
		this.Column6.Name = "Column6";
		this.Column6.ReadOnly = true;
		this.Column6.Width = 54;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1159, 553);
		base.Controls.Add(this.DG);
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MinimizeBox = false;
		base.Name = "FormLicenses";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Доступні ліцензії";
		((System.ComponentModel.ISupportInitialize)this.DG).EndInit();
		base.ResumeLayout(false);
	}

	public FormLicenses(string eTIN = "")
	{
		base.Load += FormLicenses_Load;
		AO = new AccountantОffice();
		InitializeComponent();
		TINs = eTIN;
	}

	private void FormLicenses_Load(object sender, EventArgs e)
	{
		LoadALLTin();
	}

	private void LoadALLTin()
	{
		KeysUpDateA keysUpDateA = new KeysUpDateA();
		int num = All.ArS.IndexMaxFn();
		checked
		{
			for (int i = 1; i <= num; i++)
			{
				string text = All.ArS.NameFn(i);
				string value = All.ArS.StringGetFn(text, "OrgName");
				keysUpDateA.DownloadKeyFile(text);
				int num2 = AO.CounterKeysTin(text) - 1;
				for (int j = 0; j <= num2; j++)
				{
					string text2 = All.ArS.StringGetFn(text, AO.NameKeyINI("Fn", Conversions.ToInteger(j.ToString())));
					string value2 = All.ArS.StringGetFn(text, AO.NameKeyINI("Ad", Conversions.ToInteger(j.ToString())));
					string text3 = FullVersionT(text, text2);
					int num3 = 0;
					if (Operators.CompareString(text3, "free", TextCompare: false) == 0)
					{
						num3 = 0;
						DateTime now = DateTime.Now;
					}
					else
					{
						DateTime now = Convert.ToDateTime(text3);
						num3 = (int)DateAndTime.DateDiff(DateInterval.Day, DateTime.Now, now);
					}
					DG.RowCount++;
					DG[0, DG.RowCount - 1].Value = text;
					DG[1, DG.RowCount - 1].Value = value;
					DG[2, DG.RowCount - 1].Value = text2;
					DG[3, DG.RowCount - 1].Value = value2;
					DG[4, DG.RowCount - 1].Value = text3;
					DG[5, DG.RowCount - 1].Value = RemainingDays(num3);
					if (Operators.CompareString(text3, "free", TextCompare: false) == 0)
					{
						DG.Rows[DG.RowCount - 1].DefaultCellStyle.ForeColor = Color.FromArgb(108, 108, 108);
					}
					else if (num3 < 15)
					{
						DG.Rows[DG.RowCount - 1].DefaultCellStyle.ForeColor = Color.FromArgb(81, 0, 0);
					}
					else
					{
						DG.Rows[DG.RowCount - 1].DefaultCellStyle.ForeColor = Color.FromArgb(0, 81, 0);
					}
				}
			}
		}
	}

	private string RemainingDays(int e)
	{
		string text = e.ToString();
		if (text.Length == 1)
		{
			text = "0" + text;
		}
		if (text.Length == 2)
		{
			text = "0" + text;
		}
		return text;
	}

	private string FullVersionT(string ttt, string fff)
	{
		KeysWC keysWC = new KeysWC();
		DateTime now = DateTime.Now;
		string text = now.Year.ToString();
		string text2 = now.Month.ToString();
		if (text2.Length < 2)
		{
			text2 = "0" + text2;
		}
		text += text2;
		string text3 = now.Day.ToString();
		if (text3.Length < 2)
		{
			text3 = "0" + text3;
		}
		text += text3;
		long num = keysWC.DhgbK(ttt, fff);
		if (num < Conversions.ToInteger(text))
		{
			return "free";
		}
		string text4 = num.ToString();
		return Conversions.ToString(text4[6]) + Conversions.ToString(text4[7]) + "/" + Conversions.ToString(text4[4]) + Conversions.ToString(text4[5]) + "/" + Conversions.ToString(text4[0]) + Conversions.ToString(text4[1]) + Conversions.ToString(text4[2]) + Conversions.ToString(text4[3]);
	}

	private void FnS(string eT)
	{
	}
}
