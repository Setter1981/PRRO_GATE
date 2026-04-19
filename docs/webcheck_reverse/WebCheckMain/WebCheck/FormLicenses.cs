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
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_0351: Unknown result type (might be due to invalid IL or missing references)
		//IL_035b: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormLicenses));
		DG = new DataGridView();
		Column1 = new DataGridViewTextBoxColumn();
		Column2 = new DataGridViewTextBoxColumn();
		Column3 = new DataGridViewTextBoxColumn();
		Column4 = new DataGridViewTextBoxColumn();
		Column5 = new DataGridViewTextBoxColumn();
		Column6 = new DataGridViewTextBoxColumn();
		((ISupportInitialize)DG).BeginInit();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[6]
		{
			(DataGridViewColumn)Column1,
			(DataGridViewColumn)Column2,
			(DataGridViewColumn)Column3,
			(DataGridViewColumn)Column4,
			(DataGridViewColumn)Column5,
			(DataGridViewColumn)Column6
		});
		((Control)DG).Location = new Point(2, 12);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		DG.RowHeadersWidth = 51;
		DG.RowTemplate.Height = 24;
		((Control)DG).Size = new Size(1156, 541);
		((Control)DG).TabIndex = 1;
		((DataGridViewColumn)Column1).HeaderText = "TIN";
		((DataGridViewColumn)Column1).MinimumWidth = 6;
		((DataGridViewColumn)Column1).Name = "Column1";
		((DataGridViewColumn)Column1).ReadOnly = true;
		((DataGridViewColumn)Column1).Width = 90;
		((DataGridViewColumn)Column2).HeaderText = "Назва";
		((DataGridViewColumn)Column2).MinimumWidth = 6;
		((DataGridViewColumn)Column2).Name = "Column2";
		((DataGridViewColumn)Column2).ReadOnly = true;
		((DataGridViewColumn)Column2).Width = 136;
		((DataGridViewColumn)Column3).HeaderText = "FN";
		((DataGridViewColumn)Column3).MinimumWidth = 6;
		((DataGridViewColumn)Column3).Name = "Column3";
		((DataGridViewColumn)Column3).ReadOnly = true;
		((DataGridViewColumn)Column3).Width = 81;
		((DataGridViewColumn)Column4).HeaderText = "Адреса";
		((DataGridViewColumn)Column4).MinimumWidth = 6;
		((DataGridViewColumn)Column4).Name = "Column4";
		((DataGridViewColumn)Column4).ReadOnly = true;
		((DataGridViewColumn)Column4).Width = 333;
		((DataGridViewColumn)Column5).HeaderText = "Ліцензія";
		((DataGridViewColumn)Column5).MinimumWidth = 6;
		((DataGridViewColumn)Column5).Name = "Column5";
		((DataGridViewColumn)Column5).ReadOnly = true;
		((DataGridViewColumn)Column5).Width = 90;
		((DataGridViewColumn)Column6).HeaderText = "Днів";
		((DataGridViewColumn)Column6).MinimumWidth = 6;
		((DataGridViewColumn)Column6).Name = "Column6";
		((DataGridViewColumn)Column6).ReadOnly = true;
		((DataGridViewColumn)Column6).Width = 54;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1159, 553);
		((Control)this).Controls.Add((Control)(object)DG);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormLicenses";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Доступні ліцензії";
		((ISupportInitialize)DG).EndInit();
		((Control)this).ResumeLayout(false);
	}

	public FormLicenses(string eTIN = "")
	{
		((Form)this).Load += FormLicenses_Load;
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
					if (Operators.CompareString(text3, "free", false) == 0)
					{
						num3 = 0;
						DateTime now = DateTime.Now;
					}
					else
					{
						DateTime now = Convert.ToDateTime(text3);
						num3 = (int)DateAndTime.DateDiff((DateInterval)4, DateTime.Now, now, (FirstDayOfWeek)1, (FirstWeekOfYear)1);
					}
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = text;
					DG[1, DG.RowCount - 1].Value = value;
					DG[2, DG.RowCount - 1].Value = text2;
					DG[3, DG.RowCount - 1].Value = value2;
					DG[4, DG.RowCount - 1].Value = text3;
					DG[5, DG.RowCount - 1].Value = RemainingDays(num3);
					if (Operators.CompareString(text3, "free", false) == 0)
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
