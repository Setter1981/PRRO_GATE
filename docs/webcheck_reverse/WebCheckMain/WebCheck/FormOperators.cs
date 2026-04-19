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
internal class FormOperators : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("ДодатиОператораToolStripMenuItem")]
	private ToolStripMenuItem _ДодатиОператораToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("СкопіюватиНомерСертифікатаToolStripMenuItem")]
	private ToolStripMenuItem _СкопіюватиНомерСертифікатаToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЗакритиToolStripMenuItem")]
	private ToolStripMenuItem _ЗакритиToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ВидалитиСертифікатToolStripMenuItem")]
	private ToolStripMenuItem _ВидалитиСертифікатToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("DefaultOpToolStripMenuItem")]
	private ToolStripMenuItem _DefaultOpToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ВидалитиОператоравідкладенийКлючToolStripMenuItem")]
	private ToolStripMenuItem _ВидалитиОператоравідкладенийКлючToolStripMenuItem;

	private int nr;

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
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			//IL_0014: Unknown result type (might be due to invalid IL or missing references)
			//IL_001a: Expected O, but got Unknown
			//IL_0021: Unknown result type (might be due to invalid IL or missing references)
			//IL_0027: Expected O, but got Unknown
			DataGridViewCellEventHandler val = new DataGridViewCellEventHandler(DG_CellDoubleClick);
			DataGridViewCellEventHandler val2 = new DataGridViewCellEventHandler(DG_CellContentClick);
			DataGridViewCellEventHandler val3 = new DataGridViewCellEventHandler(DG_CellClick);
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick -= val;
				dG.CellContentClick -= val2;
				dG.CellClick -= val3;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick += val;
				dG.CellContentClick += val2;
				dG.CellClick += val3;
			}
		}
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

	internal virtual ToolStripMenuItem ДодатиОператораToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДодатиОператораToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ДодатиОператораToolStripMenuItem_Click;
			ToolStripMenuItem додатиОператораToolStripMenuItem = _ДодатиОператораToolStripMenuItem;
			if (додатиОператораToolStripMenuItem != null)
			{
				((ToolStripItem)додатиОператораToolStripMenuItem).Click -= eventHandler;
			}
			_ДодатиОператораToolStripMenuItem = value;
			додатиОператораToolStripMenuItem = _ДодатиОператораToolStripMenuItem;
			if (додатиОператораToolStripMenuItem != null)
			{
				((ToolStripItem)додатиОператораToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem СкопіюватиНомерСертифікатаToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _СкопіюватиНомерСертифікатаToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = СкопіюватиНомерСертифікатаToolStripMenuItem_Click;
			ToolStripMenuItem скопіюватиНомерСертифікатаToolStripMenuItem = _СкопіюватиНомерСертифікатаToolStripMenuItem;
			if (скопіюватиНомерСертифікатаToolStripMenuItem != null)
			{
				((ToolStripItem)скопіюватиНомерСертифікатаToolStripMenuItem).Click -= eventHandler;
			}
			_СкопіюватиНомерСертифікатаToolStripMenuItem = value;
			скопіюватиНомерСертифікатаToolStripMenuItem = _СкопіюватиНомерСертифікатаToolStripMenuItem;
			if (скопіюватиНомерСертифікатаToolStripMenuItem != null)
			{
				((ToolStripItem)скопіюватиНомерСертифікатаToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ToolStripMenuItem2")]
	internal virtual ToolStripSeparator ToolStripMenuItem2
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
			EventHandler eventHandler = ЗакритиToolStripMenuItem_Click;
			ToolStripMenuItem закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				((ToolStripItem)закритиToolStripMenuItem).Click -= eventHandler;
			}
			_ЗакритиToolStripMenuItem = value;
			закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				((ToolStripItem)закритиToolStripMenuItem).Click += eventHandler;
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

	[field: AccessedThroughProperty("Column7")]
	internal virtual DataGridViewTextBoxColumn Column7
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column8")]
	internal virtual DataGridViewTextBoxColumn Column8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ToolStripMenuItem3")]
	internal virtual ToolStripSeparator ToolStripMenuItem3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ВидалитиСертифікатToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ВидалитиСертифікатToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ВидалитиСертифікатToolStripMenuItem_Click;
			ToolStripMenuItem видалитиСертифікатToolStripMenuItem = _ВидалитиСертифікатToolStripMenuItem;
			if (видалитиСертифікатToolStripMenuItem != null)
			{
				((ToolStripItem)видалитиСертифікатToolStripMenuItem).Click -= eventHandler;
			}
			_ВидалитиСертифікатToolStripMenuItem = value;
			видалитиСертифікатToolStripMenuItem = _ВидалитиСертифікатToolStripMenuItem;
			if (видалитиСертифікатToolStripMenuItem != null)
			{
				((ToolStripItem)видалитиСертифікатToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem DefaultOpToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _DefaultOpToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = DefaultOpToolStripMenuItem_Click;
			ToolStripMenuItem defaultOpToolStripMenuItem = _DefaultOpToolStripMenuItem;
			if (defaultOpToolStripMenuItem != null)
			{
				((ToolStripItem)defaultOpToolStripMenuItem).Click -= eventHandler;
			}
			_DefaultOpToolStripMenuItem = value;
			defaultOpToolStripMenuItem = _DefaultOpToolStripMenuItem;
			if (defaultOpToolStripMenuItem != null)
			{
				((ToolStripItem)defaultOpToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	internal virtual ToolStripMenuItem ВидалитиОператоравідкладенийКлючToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ВидалитиОператоравідкладенийКлючToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ВидалитиОператоравідкладенийКлючToolStripMenuItem_Click;
			ToolStripMenuItem видалитиОператоравідкладенийКлючToolStripMenuItem = _ВидалитиОператоравідкладенийКлючToolStripMenuItem;
			if (видалитиОператоравідкладенийКлючToolStripMenuItem != null)
			{
				((ToolStripItem)видалитиОператоравідкладенийКлючToolStripMenuItem).Click -= eventHandler;
			}
			_ВидалитиОператоравідкладенийКлючToolStripMenuItem = value;
			видалитиОператоравідкладенийКлючToolStripMenuItem = _ВидалитиОператоравідкладенийКлючToolStripMenuItem;
			if (видалитиОператоравідкладенийКлючToolStripMenuItem != null)
			{
				((ToolStripItem)видалитиОператоравідкладенийКлючToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	public FormOperators()
	{
		((Form)this).Load += FormOperators_Load;
		nr = 0;
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
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_009f: Expected O, but got Unknown
		//IL_00a0: Unknown result type (might be due to invalid IL or missing references)
		//IL_00aa: Expected O, but got Unknown
		//IL_00ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b5: Expected O, but got Unknown
		//IL_00b6: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c0: Expected O, but got Unknown
		//IL_00c1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00cb: Expected O, but got Unknown
		//IL_00cc: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d6: Expected O, but got Unknown
		//IL_00d7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e1: Expected O, but got Unknown
		//IL_00e2: Unknown result type (might be due to invalid IL or missing references)
		//IL_00ec: Expected O, but got Unknown
		//IL_0778: Unknown result type (might be due to invalid IL or missing references)
		//IL_0782: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormOperators));
		DG = new DataGridView();
		Column1 = new DataGridViewTextBoxColumn();
		Column2 = new DataGridViewTextBoxColumn();
		Column3 = new DataGridViewTextBoxColumn();
		Column4 = new DataGridViewTextBoxColumn();
		Column5 = new DataGridViewTextBoxColumn();
		Column6 = new DataGridViewTextBoxColumn();
		Column7 = new DataGridViewTextBoxColumn();
		Column8 = new DataGridViewTextBoxColumn();
		MenuStrip1 = new MenuStrip();
		МенюToolStripMenuItem = new ToolStripMenuItem();
		СкопіюватиНомерСертифікатаToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem3 = new ToolStripSeparator();
		ВидалитиОператоравідкладенийКлючToolStripMenuItem = new ToolStripMenuItem();
		ВидалитиСертифікатToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem1 = new ToolStripSeparator();
		ДодатиОператораToolStripMenuItem = new ToolStripMenuItem();
		DefaultOpToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem2 = new ToolStripSeparator();
		ЗакритиToolStripMenuItem = new ToolStripMenuItem();
		((ISupportInitialize)DG).BeginInit();
		((Control)MenuStrip1).SuspendLayout();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[8]
		{
			(DataGridViewColumn)Column1,
			(DataGridViewColumn)Column2,
			(DataGridViewColumn)Column3,
			(DataGridViewColumn)Column4,
			(DataGridViewColumn)Column5,
			(DataGridViewColumn)Column6,
			(DataGridViewColumn)Column7,
			(DataGridViewColumn)Column8
		});
		((Control)DG).Location = new Point(3, 31);
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		DG.RowHeadersWidth = 51;
		DG.RowTemplate.Height = 24;
		((Control)DG).Size = new Size(1103, 416);
		((Control)DG).TabIndex = 0;
		((DataGridViewColumn)Column1).HeaderText = "№";
		((DataGridViewColumn)Column1).MinimumWidth = 6;
		((DataGridViewColumn)Column1).Name = "Column1";
		((DataGridViewColumn)Column1).ReadOnly = true;
		((DataGridViewColumn)Column1).Width = 50;
		((DataGridViewColumn)Column2).HeaderText = "ПІБ";
		((DataGridViewColumn)Column2).MinimumWidth = 6;
		((DataGridViewColumn)Column2).Name = "Column2";
		((DataGridViewColumn)Column2).ReadOnly = true;
		((DataGridViewColumn)Column2).Width = 180;
		((DataGridViewColumn)Column3).HeaderText = "Шлях до файлу ключа";
		((DataGridViewColumn)Column3).MinimumWidth = 6;
		((DataGridViewColumn)Column3).Name = "Column3";
		((DataGridViewColumn)Column3).ReadOnly = true;
		((DataGridViewColumn)Column3).Width = 174;
		((DataGridViewColumn)Column4).HeaderText = "Пароль";
		((DataGridViewColumn)Column4).MinimumWidth = 6;
		((DataGridViewColumn)Column4).Name = "Column4";
		((DataGridViewColumn)Column4).ReadOnly = true;
		((DataGridViewColumn)Column4).Width = 75;
		((DataGridViewColumn)Column5).HeaderText = "ІНН";
		((DataGridViewColumn)Column5).MinimumWidth = 6;
		((DataGridViewColumn)Column5).Name = "Column5";
		((DataGridViewColumn)Column5).ReadOnly = true;
		((DataGridViewColumn)Column5).Width = 90;
		((DataGridViewColumn)Column6).HeaderText = "Сертифікат";
		((DataGridViewColumn)Column6).MinimumWidth = 6;
		((DataGridViewColumn)Column6).Name = "Column6";
		((DataGridViewColumn)Column6).ReadOnly = true;
		((DataGridViewColumn)Column6).Width = 125;
		((DataGridViewColumn)Column7).HeaderText = "Початок дії";
		((DataGridViewColumn)Column7).MinimumWidth = 6;
		((DataGridViewColumn)Column7).Name = "Column7";
		((DataGridViewColumn)Column7).ReadOnly = true;
		((DataGridViewColumn)Column7).Width = 117;
		((DataGridViewColumn)Column8).HeaderText = "Кінець дії";
		((DataGridViewColumn)Column8).MinimumWidth = 6;
		((DataGridViewColumn)Column8).Name = "Column8";
		((DataGridViewColumn)Column8).ReadOnly = true;
		((DataGridViewColumn)Column8).Width = 108;
		((ToolStrip)MenuStrip1).ImageScalingSize = new Size(20, 20);
		((ToolStrip)MenuStrip1).Items.AddRange((ToolStripItem[])(object)new ToolStripItem[1] { (ToolStripItem)МенюToolStripMenuItem });
		((Control)MenuStrip1).Location = new Point(0, 0);
		((Control)MenuStrip1).Name = "MenuStrip1";
		((Control)MenuStrip1).Size = new Size(1111, 28);
		((Control)MenuStrip1).TabIndex = 1;
		((Control)MenuStrip1).Text = "MenuStrip1";
		((ToolStripDropDownItem)МенюToolStripMenuItem).DropDownItems.AddRange((ToolStripItem[])(object)new ToolStripItem[9]
		{
			(ToolStripItem)СкопіюватиНомерСертифікатаToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem3,
			(ToolStripItem)ВидалитиОператоравідкладенийКлючToolStripMenuItem,
			(ToolStripItem)ВидалитиСертифікатToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem1,
			(ToolStripItem)ДодатиОператораToolStripMenuItem,
			(ToolStripItem)DefaultOpToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem2,
			(ToolStripItem)ЗакритиToolStripMenuItem
		});
		((ToolStripItem)МенюToolStripMenuItem).Name = "МенюToolStripMenuItem";
		((ToolStripItem)МенюToolStripMenuItem).Size = new Size(65, 24);
		((ToolStripItem)МенюToolStripMenuItem).Text = "Меню";
		((ToolStripItem)СкопіюватиНомерСертифікатаToolStripMenuItem).Name = "СкопіюватиНомерСертифікатаToolStripMenuItem";
		((ToolStripItem)СкопіюватиНомерСертифікатаToolStripMenuItem).Size = new Size(385, 26);
		((ToolStripItem)СкопіюватиНомерСертифікатаToolStripMenuItem).Text = "Скопіювати номер сертифіката ";
		((ToolStripItem)ToolStripMenuItem3).Name = "ToolStripMenuItem3";
		((ToolStripItem)ToolStripMenuItem3).Size = new Size(382, 6);
		((ToolStripItem)ВидалитиОператоравідкладенийКлючToolStripMenuItem).Name = "ВидалитиОператоравідкладенийКлючToolStripMenuItem";
		((ToolStripItem)ВидалитиОператоравідкладенийКлючToolStripMenuItem).Size = new Size(385, 26);
		((ToolStripItem)ВидалитиОператоравідкладенийКлючToolStripMenuItem).Text = "Видалити оператора (відкладений ключ)...";
		((ToolStripItem)ВидалитиСертифікатToolStripMenuItem).Name = "ВидалитиСертифікатToolStripMenuItem";
		((ToolStripItem)ВидалитиСертифікатToolStripMenuItem).Size = new Size(385, 26);
		((ToolStripItem)ВидалитиСертифікатToolStripMenuItem).Text = "Видалити сертифікат";
		((ToolStripItem)ToolStripMenuItem1).Name = "ToolStripMenuItem1";
		((ToolStripItem)ToolStripMenuItem1).Size = new Size(382, 6);
		((ToolStripItem)ДодатиОператораToolStripMenuItem).Name = "ДодатиОператораToolStripMenuItem";
		((ToolStripItem)ДодатиОператораToolStripMenuItem).Size = new Size(385, 26);
		((ToolStripItem)ДодатиОператораToolStripMenuItem).Text = "Додати оператора...";
		((ToolStripItem)DefaultOpToolStripMenuItem).Name = "DefaultOpToolStripMenuItem";
		((ToolStripItem)DefaultOpToolStripMenuItem).Size = new Size(385, 26);
		((ToolStripItem)DefaultOpToolStripMenuItem).Text = "Оператор за замовчуванням";
		((ToolStripItem)ToolStripMenuItem2).Name = "ToolStripMenuItem2";
		((ToolStripItem)ToolStripMenuItem2).Size = new Size(382, 6);
		((ToolStripItem)ЗакритиToolStripMenuItem).Name = "ЗакритиToolStripMenuItem";
		((ToolStripItem)ЗакритиToolStripMenuItem).Size = new Size(385, 26);
		((ToolStripItem)ЗакритиToolStripMenuItem).Text = "Закрити";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1111, 453);
		((Control)this).Controls.Add((Control)(object)DG);
		((Control)this).Controls.Add((Control)(object)MenuStrip1);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Form)this).MinimumSize = new Size(900, 300);
		((Control)this).Name = "FormOperators";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Оператори";
		((ISupportInitialize)DG).EndInit();
		((Control)MenuStrip1).ResumeLayout(false);
		((Control)MenuStrip1).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormOperators_Load(object sender, EventArgs e)
	{
		((ToolStripItem)ВидалитиСертифікатToolStripMenuItem).Visible = false;
		LoadOperators();
		Application.DoEvents();
	}

	private void LoadOperators()
	{
		checked
		{
			try
			{
				DG.RowCount = 0;
				OperatorsAll operatorsAll = new OperatorsAll();
				if (operatorsAll.Operators < 1)
				{
					nr = -1;
				}
				Coding coding = new Coding();
				int operators = operatorsAll.Operators;
				for (int i = 1; i <= operators; i++)
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = operatorsAll.get_Seller(0, i);
					DG[1, DG.RowCount - 1].Value = operatorsAll.get_Seller(1, i);
					DG[2, DG.RowCount - 1].Value = operatorsAll.get_Seller(2, i);
					DG[3, DG.RowCount - 1].Value = "*********";
					DG[4, DG.RowCount - 1].Value = operatorsAll.get_Seller(4, i);
					string section = operatorsAll.get_Seller(4, i);
					IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini");
					string value = DateTime.Now.Day.ToString();
					TypErrStrCert typErrStrCert = All.SF.Cert(operatorsAll.get_Seller(2, i), coding.DeCod(operatorsAll.get_Seller(3, i)));
					if (typErrStrCert.errCode == 0)
					{
						DG[5, DG.RowCount - 1].Value = typErrStrCert.ReturnStr;
						DG[6, DG.RowCount - 1].Value = typErrStrCert.ReturnStart;
						DG[7, DG.RowCount - 1].Value = typErrStrCert.ReturnEnd;
						iniHGB.WriteString(section, "StartKey", typErrStrCert.ReturnStart);
						iniHGB.WriteString(section, "EndKey", typErrStrCert.ReturnEnd);
						iniHGB.WriteString(section, "Issuer", typErrStrCert.ReturnIssuer);
						iniHGB.WriteString(section, "Serial", typErrStrCert.ReturnSerial);
						iniHGB.WriteString(section, "Updated", value);
					}
					else
					{
						iniHGB.WriteString(section, "Updated", "0");
						All.Lg.SaveTextToLog("Оператор: " + operatorsAll.get_Seller(1, i), "Ошибка: " + typErrStrCert.errCode, "Описание ошибки: " + typErrStrCert.errStr);
					}
				}
				if (operatorsAll.Operators > 1)
				{
					DG.Rows[0].DefaultCellStyle.BackColor = Color.FromArgb(227, 255, 255);
					DefaultOpToolStripMenuItem.Enabled = false;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("Форма Операторы", "Ошибка заполнения таблицы с операторами", "Критическая ошибка");
				ProjectData.ClearProjectError();
			}
		}
	}

	private void ДодатиОператораToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_001f: Unknown result type (might be due to invalid IL or missing references)
		((Form)new FormOperator(NewOperator: true)).ShowDialog();
		LoadOperators();
	}

	private void DG_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		OperatorsAll operatorsAll = new OperatorsAll();
		int y = checked(e.RowIndex + 1);
		((Form)new FormOperator(NewOperator: false, operatorsAll.get_Seller(0, y), operatorsAll.get_Seller(1, y), operatorsAll.get_Seller(4, y), operatorsAll.get_Seller(3, y), operatorsAll.get_Seller(2, y))).ShowDialog();
		LoadOperators();
	}

	private void DG_CellContentClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			nr = e.RowIndex;
		}
		if (Conversions.ToInteger(DG[0, nr].Value.ToString()) > 1)
		{
			DefaultOpToolStripMenuItem.Enabled = true;
		}
		else
		{
			DefaultOpToolStripMenuItem.Enabled = false;
		}
	}

	private void ЗакритиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void СкопіюватиНомерСертифікатаToolStripMenuItem_Click(object sender, EventArgs e)
	{
		try
		{
			Clipboard.SetText(DG[5, nr].Value.ToString());
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Clipboard.SetText("");
			ProjectData.ClearProjectError();
		}
	}

	private void DG_CellClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			nr = e.RowIndex;
		}
		if (Conversions.ToInteger(DG[0, nr].Value.ToString()) > 1)
		{
			DefaultOpToolStripMenuItem.Enabled = true;
		}
		else
		{
			DefaultOpToolStripMenuItem.Enabled = false;
		}
	}

	private void ВидалитиСертифікатToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_007a: Unknown result type (might be due to invalid IL or missing references)
		//IL_005b: Unknown result type (might be due to invalid IL or missing references)
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		try
		{
			string tINN = DG[4, nr].Value.ToString();
			typErr = All.SF.CertDel(tINN);
			if (typErr.errCode == 0)
			{
				LoadOperators();
				Application.DoEvents();
				Interaction.MsgBox((object)"Сертифікат видалено!", (MsgBoxStyle)0, (object)"Видалення сертифікату");
			}
			else
			{
				Interaction.MsgBox((object)("Помилка видалення сертифіката: " + typErr.errStr), (MsgBoxStyle)16, (object)"Увага!");
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Interaction.MsgBox((object)"Помилка видалення сертифіката", (MsgBoxStyle)16, (object)"Увага!");
			ProjectData.ClearProjectError();
		}
	}

	private void DefaultOpToolStripMenuItem_Click(object sender, EventArgs e)
	{
		int num = Conversions.ToInteger(DG[0, nr].Value.ToString());
		if (num > 1)
		{
			try
			{
				new OperatorsAll().OperatorDefault(num);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("Виникла помилка при установці оператора за замовчуванням: ", ex2.Message);
				ProjectData.ClearProjectError();
			}
			LoadOperators();
			Application.DoEvents();
		}
		else
		{
			DefaultOpToolStripMenuItem.Enabled = false;
		}
	}

	private void ВидалитиОператоравідкладенийКлючToolStripMenuItem_Click(object sender, EventArgs e)
	{
		//IL_0018: Unknown result type (might be due to invalid IL or missing references)
		//IL_0042: Unknown result type (might be due to invalid IL or missing references)
		//IL_009d: Unknown result type (might be due to invalid IL or missing references)
		//IL_00a3: Invalid comparison between Unknown and I4
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		if (All.l.OfflineTrue())
		{
			Interaction.MsgBox((object)"Для видалення оператора необхідно вийти з офлайн режиму", (MsgBoxStyle)48, (object)"Увага");
			return;
		}
		if (Conversions.ToInteger(All.l.ReturnOpenShift().ReturnStr) > 0)
		{
			Interaction.MsgBox((object)"Для видалення оператора необхідно закрити зміну", (MsgBoxStyle)48, (object)"Увага");
			return;
		}
		int index = ((DataGridViewBand)DG.CurrentRow).Index;
		if (Operators.CompareString(DG[0, index].Value.ToString(), "1", false) == 0)
		{
			Interaction.MsgBox((object)"Першого оператора видаляти не можна", (MsgBoxStyle)48, (object)"Увага");
		}
		else if ((int)Interaction.MsgBox((object)"Ви дійсно хочете видалити вибраного оператора (відкладений ключ)?", (MsgBoxStyle)52, (object)"Увага") != 7)
		{
			string text = DG[4, index].Value.ToString();
			UpdateKeys updateKeys = new UpdateKeys();
			updateKeys.DelOp(text.ToString());
			updateKeys.DelKey(text.ToString());
			LoadOperators();
		}
	}
}
